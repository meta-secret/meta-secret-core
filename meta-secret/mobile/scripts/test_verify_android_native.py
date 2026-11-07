"""Artifact fixtures for the Android native-layout gate; no SDK/compiler required."""
import importlib.util
import io
from pathlib import Path
import struct
import tempfile
import unittest
import warnings
import zipfile


SPEC = importlib.util.spec_from_file_location(
    "verify_android_native", Path(__file__).with_name("verify-android-native.py"))
verifier = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verifier)

ABIS = {"arm64-v8a": (2, 183), "x86_64": (2, 62),
        "armeabi-v7a": (1, 40), "x86": (1, 3), "armeabi": (1, 40),
        "mips": (1, 8), "mips64": (2, 8)}
OWN = "libmetasecret_mobile.so"


def elf(abi="arm64-v8a", align=16384, offset=0, address=0,
        load_flags=5, stack_flags=6, relro=True, load=True, relro_size=256):
    """Construct actual minimal ELF headers/program tables, not parser mocks."""
    cls, machine = ABIS[abi]
    ident = b"\x7fELF" + bytes([cls, 1, 1, 0]) + bytes(8)
    segments = []
    if load:
        segments.append((1, load_flags, offset, address, 0, 0, 4096, align))
    segments.append((0x6474E551, stack_flags, 0, 0, 0, 0, 0, 16))
    if relro:
        segments.append((0x6474E552, 4, offset, address, 0, 0, relro_size, 1))
    if cls == 2:
        header = struct.pack("<HHIQQQIHHHHHH", 3, machine, 1, 0, 64, 0,
                             0, 64, 56, len(segments), 0, 0, 0)
        table = b"".join(struct.pack("<IIQQQQQQ", *s) for s in segments)
    else:
        header = struct.pack("<HHIIIIIHHHHHH", 3, machine, 1, 0, 52, 0,
                             0, 52, 32, len(segments), 0, 0, 0)
        table = b"".join(struct.pack("<IIIIIIII", s[0], s[2], s[3],
                                        s[4], s[5], s[6], s[1], s[7])
                         for s in segments)
    return ident + header + table


def apk_bytes(entries, aligned=True, compression=zipfile.ZIP_STORED):
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w") as archive:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            for name, payload in entries:
                info = zipfile.ZipInfo(name)
                info.compress_type = compression
                # A real local extra-field pads the DATA offset, not the header.
                if aligned:
                    pad = (-(stream.tell() + 30 + len(name.encode()) + 4)) % 16384
                    info.extra = struct.pack("<HH", 0xCAFE, pad) + bytes(pad)
                archive.writestr(info, payload)
    return stream.getvalue()


def own_entries():
    return [(f"lib/{abi}/{OWN}", elf(abi))
            for abi in ("arm64-v8a", "armeabi-v7a", "x86_64")]


class NativeLayoutTests(unittest.TestCase):
    def inspect_apk(self, data, role="app"):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fixture.apk"
            path.write_bytes(data)
            return verifier.inspect_apk(path, role=role)

    def reject_elf(self, data, abi="arm64-v8a"):
        with self.assertRaises(verifier.VerificationError):
            verifier.inspect_elf(data, abi)

    def reject_apk(self, entries, **kwargs):
        with self.assertRaises(verifier.VerificationError):
            self.inspect_apk(apk_bytes(entries, **kwargs))

    def test_valid_arm64(self):
        result = verifier.inspect_elf(elf(), "arm64-v8a")
        self.assertEqual(result["abi"], "arm64-v8a")
        self.assertTrue(result["requires_16k"])
        self.assertEqual(len(result["loads"]), 1)

    def test_valid_x86_64(self):
        self.assertTrue(verifier.inspect_elf(elf("x86_64"), "x86_64")["requires_16k"])

    def test_overaligned_power_of_two(self):
        verifier.inspect_elf(elf(align=65536), "arm64-v8a")

    def test_arm64_4096_rejected(self):
        self.reject_elf(elf(align=4096))

    def test_x86_64_4096_rejected(self):
        self.reject_elf(elf("x86_64", align=4096), "x86_64")

    def test_legacy_abis_are_classified_not_required_16k(self):
        for abi in ("armeabi-v7a", "armeabi", "x86", "mips", "mips64"):
            with self.subTest(abi=abi):
                result = verifier.inspect_elf(elf(abi, align=4096), abi)
                self.assertEqual(result["abi"], abi)
                self.assertFalse(result["requires_16k"])

    def test_nonpower2_alignment_rejected(self):
        self.reject_elf(elf(align=24576))

    def test_zero_alignment_rejected(self):
        self.reject_elf(elf(align=0))

    def test_offset_address_incongruent_rejected(self):
        self.reject_elf(elf(offset=4096))

    def test_nonzero_congruent_offset_address_accepted(self):
        verifier.inspect_elf(elf(offset=0, address=16384), "arm64-v8a")

    def test_wrong_machine_rejected(self):
        self.reject_elf(elf("x86_64"))

    def test_wrong_class_rejected(self):
        self.reject_elf(elf("armeabi-v7a"))

    def test_unknown_abi_rejected(self):
        self.reject_elf(elf(), "unknown")

    def test_bad_magic_rejected(self):
        self.reject_elf(b"BAD!" + elf()[4:])

    def test_truncated_header_rejected(self):
        self.reject_elf(elf()[:24])

    def test_truncated_program_table_rejected(self):
        self.reject_elf(elf()[:-1])

    def test_invalid_program_table_offset_rejected(self):
        data = bytearray(elf())
        struct.pack_into("<Q", data, 32, 0xFFFFFFFF)
        self.reject_elf(bytes(data))

    def test_zero_load_segments_rejected(self):
        self.reject_elf(elf(load=False))

    def test_unsupported_endianness_rejected(self):
        data = bytearray(elf())
        data[5] = 0
        self.reject_elf(bytes(data))

    def test_fully_big_endian_android_elf_rejected(self):
        little = elf()
        ident = bytearray(little[:16])
        ident[5] = 2
        header = struct.pack(">HHIQQQIHHHHHH",
                             *struct.unpack_from("<HHIQQQIHHHHHH", little, 16))
        table = b"".join(struct.pack(">IIQQQQQQ",
                                     *struct.unpack_from("<IIQQQQQQ", little, offset))
                         for offset in (64, 120, 176))
        self.reject_elf(bytes(ident) + header + table)

    def test_app_requires_all_three_own_abis(self):
        self.reject_apk(own_entries()[:-1])

    def test_empty_app_rejected(self):
        self.reject_apk([])

    def test_instrumentation_zero_native_is_valid(self):
        result = self.inspect_apk(apk_bytes([]), role="instrumentation")
        self.assertEqual(result["entries"], [])

    def test_unknown_role_rejected(self):
        with self.assertRaises(verifier.VerificationError):
            self.inspect_apk(apk_bytes([]), role="unknown")

    def test_all_26_native_entries_seven_abi_paths_are_scanned(self):
        counts = {"arm64-v8a": 6, "armeabi-v7a": 6, "x86_64": 6,
                  "x86": 5, "armeabi": 1, "mips": 1, "mips64": 1}
        entries = own_entries()
        for abi, count in counts.items():
            own_count = int(abi in ("arm64-v8a", "armeabi-v7a", "x86_64"))
            entries.extend((f"lib/{abi}/libother{i}.so", elf(abi))
                           for i in range(count - own_count))
        result = self.inspect_apk(apk_bytes(entries))
        self.assertEqual(len(result["entries"]), 26)
        self.assertEqual({e["abi"] for e in result["entries"]}, set(ABIS))

    def test_duplicate_any_native_path_rejected(self):
        extra = ("lib/x86/libother.so", elf("x86"))
        self.reject_apk(own_entries() + [extra, extra])

    def test_duplicate_own_native_path_rejected(self):
        self.reject_apk(own_entries() + [own_entries()[0]])

    def test_abi_path_machine_mismatch_rejected(self):
        entries = own_entries()
        entries[0] = (entries[0][0], elf("x86_64"))
        self.reject_apk(entries)

    def test_other_packaged_library_is_not_skipped(self):
        self.reject_apk(own_entries() + [("lib/arm64-v8a/libbad.so", elf(align=4096))])

    def test_zip_data_offset_not_header_offset(self):
        result = self.inspect_apk(apk_bytes(own_entries()))
        self.assertTrue(all(e["zip_aligned_16k"] for e in result["entries"]))
        self.assertTrue(all(e["data_offset"] % 16384 == 0 for e in result["entries"]))

    def test_misaligned_stored_zip_rejected(self):
        self.reject_apk(own_entries(), aligned=False)

    def test_compressed_entries_explicit_not_mmap_pass(self):
        result = self.inspect_apk(apk_bytes(own_entries(), compression=zipfile.ZIP_DEFLATED))
        self.assertTrue(all(not e["stored"] for e in result["entries"]))
        self.assertTrue(all(e["zip_aligned_16k"] is None for e in result["entries"]))

    def test_bad_local_zip_header_rejected(self):
        data = bytearray(apk_bytes(own_entries()))
        data[:4] = b"BAD!"
        with self.assertRaises(verifier.VerificationError):
            self.inspect_apk(bytes(data))

    def test_truncated_zip_rejected(self):
        with self.assertRaises(verifier.VerificationError):
            self.inspect_apk(apk_bytes(own_entries())[:40])

    def test_unchanged_security_is_valid(self):
        before = verifier.inspect_elf(elf(), "arm64-v8a")
        verifier.compare_security(before, before)

    def test_new_writable_executable_load_rejected(self):
        before = verifier.inspect_elf(elf(), "arm64-v8a")
        after = verifier.inspect_elf(elf(load_flags=7), "arm64-v8a")
        with self.assertRaises(verifier.VerificationError):
            verifier.compare_security(before, after)

    def test_new_executable_stack_rejected(self):
        before = verifier.inspect_elf(elf(), "arm64-v8a")
        after = verifier.inspect_elf(elf(stack_flags=7), "arm64-v8a")
        with self.assertRaises(verifier.VerificationError):
            verifier.compare_security(before, after)

    def test_relro_removal_rejected(self):
        before = verifier.inspect_elf(elf(), "arm64-v8a")
        after = verifier.inspect_elf(elf(relro=False), "arm64-v8a")
        with self.assertRaises(verifier.VerificationError):
            verifier.compare_security(before, after)

    def test_present_relro_zero_or_reduced_protection_rejected(self):
        before = verifier.inspect_elf(elf(), "arm64-v8a")
        for size in (0, 128):
            with self.subTest(relro_size=size):
                after = verifier.inspect_elf(elf(relro_size=size), "arm64-v8a")
                with self.assertRaises(verifier.VerificationError):
                    verifier.compare_security(before, after)

    def page_tail_fixture(self, relro_memsz=960, relro_address=23616):
        data = bytearray(elf(offset=7232, address=23616))
        data.extend(bytes(8144 - len(data)))
        struct.pack_into("<IIQQQQQQ", data, 64, 1, 6, 7232, 23616, 0, 912, 912, 16384)
        struct.pack_into("<IIQQQQQQ", data, 176, 0x6474E552, 4, 7232,
                         relro_address, 0, 912, relro_memsz, 1)
        return bytes(data)

    def test_realistic_page_rounded_relro_counts_actual_memory_only(self):
        result = verifier.inspect_elf(self.page_tail_fixture(), "arm64-v8a")
        self.assertEqual(result["security"]["relro_bytes"], 912)

    def test_relro_past_4096_page_tail_rejected(self):
        self.reject_elf(self.page_tail_fixture(relro_memsz=961))

    def test_disjoint_relro_rejected(self):
        self.reject_elf(self.page_tail_fixture(relro_address=32768))

    def test_relro_uncovered_file_bytes_rejected(self):
        data = bytearray(self.page_tail_fixture())
        struct.pack_into("<Q", data, 176 + 32, 913)
        data.append(0)
        self.reject_elf(bytes(data))

    def test_relro_address_overflow_rejected(self):
        self.reject_elf(self.page_tail_fixture(relro_address=(1 << 64) - 128))


if __name__ == "__main__":
    unittest.main()
