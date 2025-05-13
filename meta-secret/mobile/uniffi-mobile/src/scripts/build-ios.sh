#!/bin/bash
set -e

# Переходим в корневую директорию проекта meta-secret
cd "$(dirname "$0")/../../../.."

echo "=== Шаг 1: Сборка Rust библиотеки для iOS ==="
cargo build --release --target aarch64-apple-ios -p uniffi-mobile

echo "=== Шаг 2: Генерация Swift биндингов ==="
# Создаем директорию для Swift биндингов
mkdir -p target/swift

# Генерируем Swift биндинги с использованием файла конфигурации
cargo run -p uniffi-bindgen -- generate \
  --library target/aarch64-apple-ios/release/libmeta_core_mobile.dylib \
  --language swift \
  --out-dir target/swift \
  --config meta-secret/mobile/uniffi-mobile/uniffi.toml

echo "=== Шаг 3: Компиляция Swift модуля для iOS ==="
# Создаем директорию для Swift-модуля
mkdir -p target/swift-module-ios

# Компилируем Swift-модуль с явным указанием заголовочных файлов для iOS
xcrun --sdk iphoneos swiftc \
  -module-name MetaCore \
  -emit-library -o target/swift-module-ios/libMetaCore.dylib \
  -emit-module -emit-module-path target/swift-module-ios/ \
  -parse-as-library \
  -target arm64-apple-ios14.0 \
  -L ./target/aarch64-apple-ios/release/ \
  -lmeta_core_mobile \
  -I target/swift \
  -import-objc-header target/swift/MetaCoreFFI.h \
  target/swift/MetaCore.swift

echo "=== Готово! ==="
echo "Swift биндинги успешно сгенерированы в директории target/swift"
echo "Swift модуль для iOS успешно скомпилирован в директории target/swift-module-ios"
echo "Теперь вы можете использовать его в iOS проекте" 