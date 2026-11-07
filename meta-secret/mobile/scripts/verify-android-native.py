"""Read-only Android ELF/ZIP layout checks. No SDK or third-party modules."""
import argparse
import json
from pathlib import Path
import struct
import sys
import zipfile


class VerificationError(ValueError):
    pass


ABIS = {"arm64-v8a": (2, 183), "x86_64": (2, 62),
        "armeabi-v7a": (1, 40), "x86": (1, 3), "armeabi": (1, 40),
        "mips": (1, 8), "mips64": (2, 8)}
REQUIRED = {"arm64-v8a", "armeabi-v7a", "x86_64"}
OWN = "libmetasecret_mobile.so"


def require(condition, message):
    if not condition:
        raise VerificationError(message)


def unpack(data, offset, fmt):
    size = struct.calcsize(fmt)
    require(0 <= offset <= len(data) - size, "Truncated header/table")
    return struct.unpack_from(fmt, data, offset)


def inspect_elf(data, abi):
    require(abi in ABIS, "Unsupported ABI")
    require(len(data) >= 16 and data[:4] == b"\x7fELF", "Invalid ELF magic/header")
    cls, encoding = data[4:6]
    require(cls == ABIS[abi][0], "ELF class does not match ABI")
    require(encoding == 1 and data[6] == 1, "Unsupported Android ELF encoding/version")
    endian = "<"
    if cls == 2:
        header = unpack(data, 16, endian + "HHIQQQIHHHHHH")
        expected_header, expected_entry = 64, 56
    else:
        header = unpack(data, 16, endian + "HHIIIIIHHHHHH")
        expected_header, expected_entry = 52, 32
    kind, machine, version, _, phoff, _, _, ehsize, phsize, phnum, *_ = header
    require(kind == 3 and version == 1, "Expected shared ELF version1")
    require(machine == ABIS[abi][1], "ELF machine does not match ABI")
    require(ehsize == expected_header and phsize == expected_entry,
            "Unsupported ELF header sizes")
    require(phnum > 0 and phoff >= ehsize and phoff + phnum * phsize <= len(data),
            "Invalid program table bounds")
    required = abi in ("arm64-v8a", "x86_64")
    loads, relro, stacks = [], [], []
    for index in range(phnum):
        position = phoff + index * phsize
        if cls == 2:
            typ, flags, offset, address, _, filesz, memsz, align = unpack(
                data, position, endian + "IIQQQQQQ")
        else:
            typ, offset, address, _, filesz, memsz, flags, align = unpack(
                data, position, endian + "IIIIIIII")
        require(filesz <= memsz and offset <= len(data) and filesz <= len(data) - offset,
                "Invalid segment file/memory bounds")
        require(address + memsz < (1 << (64 if cls == 2 else 32)),
                "Segment address overflow")
        if typ == 1:
            require(align > 0 and align & (align - 1) == 0,
                    "Invalid PT_LOAD alignment")
            require((address - offset) % align == 0, "PT_LOAD alignment incongruence")
            if required:
                require(align >= 16384 and (address - offset) % 16384 == 0,
                        "PT_LOAD is not 16KiB compatible")
            loads.append({"offset": offset, "address": address, "filesz": filesz,
                          "memsz": memsz, "align": align, "flags": flags})
        elif typ == 0x6474E551:
            stacks.append(flags)
        elif typ == 0x6474E552:
            relro.append({"address": address, "memsz": memsz, "filesz": filesz,
                          "offset": offset})
    require(loads, "No PT_LOAD segments")
    # Keep ranges for an address-independent protection comparison after linking.
    coverage = []
    for protected in relro:
        start, end = protected["address"], protected["address"] + protected["memsz"]
        candidates = [load for load in loads
                      if load["address"] <= start < load["address"] + load["memsz"]]
        require(candidates, "GNU_RELRO start outside PT_LOAD memory")
        require(any(end <= (load["address"] + load["memsz"] + 4095) // 4096 * 4096
                    and start + protected["filesz"] <= load["address"] + load["memsz"]
                    and load["offset"] <= protected["offset"]
                    and protected["offset"] + protected["filesz"] <= load["offset"] + load["filesz"]
                    for load in candidates), "GNU_RELRO exceeds covered bytes/page tail")
        for load in loads:
            covered_start = max(start, load["address"])
            covered_end = min(end, load["address"] + load["memsz"])
            if covered_end > covered_start:
                coverage.append((covered_start, covered_end))
    merged = []
    for start, end in sorted(coverage):
        if merged and start <= merged[-1][1]:
            merged[-1][1] = max(end, merged[-1][1])
        else:
            merged.append([start, end])
    return {"abi": abi, "requires_16k": required, "loads": loads,
            "security": {"wx_loads": sum((load["flags"] & 3) == 3 for load in loads),
                         "stack_present": bool(stacks),
                         "executable_stack": any(flags & 1 for flags in stacks),
                         "relro_bytes": sum(end - start for start, end in merged)}}


def compare_security(before, after):
    require(before["abi"] == after["abi"], "Security comparison ABI mismatch")
    old, new = before["security"], after["security"]
    require(new["wx_loads"] <= old["wx_loads"], "New writable-executable PT_LOAD")
    if old["stack_present"] and not old["executable_stack"]:
        require(new["stack_present"] and not new["executable_stack"], "Lost NX stack")
    require(not new["executable_stack"] or old["executable_stack"],
            "New executable stack")
    require(new["relro_bytes"] >= old["relro_bytes"], "Reduced GNU_RELRO protection")


def inspect_apk(path, role="app"):
    require(role in ("app", "instrumentation"), "Unsupported APK role")
    try:
        data = Path(path).read_bytes()
        entries, seen, own = [], set(), set()
        with zipfile.ZipFile(path) as archive:
            for info in archive.infolist():
                name = info.filename
                if not (name.startswith("lib/") and name.endswith(".so")):
                    continue
                require(name not in seen, "Duplicate native entry path")
                seen.add(name)
                parts = name.split("/")
                require(len(parts) == 3 and parts[1] in ABIS and parts[2] not in ("", ".", ".."),
                        "Invalid native entry path")
                abi = parts[1]
                local = unpack(data, info.header_offset, "<IHHHHHIIIHH")
                require(local[0] == 0x04034B50, "Invalid ZIP local header")
                require(local[3] == info.compress_type, "ZIP compression mismatch")
                name_length, extra_length = local[-2:]
                name_start = info.header_offset + 30
                payload_offset = name_start + name_length + extra_length
                require(payload_offset <= len(data) and info.compress_size <= len(data) - payload_offset,
                        "ZIP local data bounds")
                encoding = "utf-8" if local[2] & 0x800 else "cp437"
                require(data[name_start:name_start + name_length].decode(encoding) == name,
                        "ZIP local filename mismatch")
                stored = info.compress_type == zipfile.ZIP_STORED
                aligned = payload_offset % 16384 == 0 if stored else None
                require(not stored or aligned, "Stored native ZIP data not 16KiB aligned")
                report = inspect_elf(archive.read(info), abi)
                report.update({"path": name, "stored": stored, "data_offset": payload_offset,
                               "zip_aligned_16k": aligned})
                entries.append(report)
                if parts[2] == OWN:
                    require(abi in REQUIRED, "Unexpected own library ABI")
                    own.add(abi)
        require(role != "app" or own == REQUIRED, "App must contain all three own ABIs")
        return {"role": role, "entries": entries}
    except (OSError, zipfile.BadZipFile, UnicodeError, RuntimeError, NotImplementedError) as error:
        raise VerificationError("Unreadable/corrupt APK: " + type(error).__name__) from error


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--role", choices=("app", "instrumentation"))
    mode.add_argument("--abi", choices=tuple(ABIS))
    parser.add_argument("path", type=Path)
    args = parser.parse_args()
    try:
        result = (inspect_apk(args.path, args.role) if args.role else
                  inspect_elf(args.path.read_bytes(), args.abi))
        print(json.dumps(result, sort_keys=True))
        return 0
    except (VerificationError, OSError) as error:
        print(json.dumps({"error": str(error)}, sort_keys=True), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
