#!/bin/bash
set -e

# Переход к корню проекта
cd "$(dirname "$0")/.."

# Проверяем, установлен ли cbindgen, и устанавливаем его при необходимости
if ! command -v cbindgen &> /dev/null; then
    echo "Installing cbindgen..."
    cargo install cbindgen
fi

# Путь к корню проекта meta-secret
ROOT_DIR="$(cd .. && pwd)"

# Установка необходимых таргетов для iOS
rustup target add aarch64-apple-ios x86_64-apple-ios

# Переходим в директорию iOS
cd ios

echo "Compilation for iOS (arm64)..."
cargo build --package mobile-ios --target aarch64-apple-ios --release

echo "Compilation for iOS simulator (x86_64)..."
cargo build --package mobile-ios --target x86_64-apple-ios --release

echo "Creating universal lib..."
mkdir -p "${ROOT_DIR}/target/ios/universal/release"
lipo -create \
  "${ROOT_DIR}/target/aarch64-apple-ios/release/libmobile.a" \
  "${ROOT_DIR}/target/x86_64-apple-ios/release/libmobile.a" \
  -output "${ROOT_DIR}/target/ios/universal/release/metasecret-mobile.a"

echo "Generating header file..."
HEADER_OUTPUT_DIR="${ROOT_DIR}/target/ios/universal/release"
HEADER_FILE="${HEADER_OUTPUT_DIR}/metasecret-mobile.h"

# Создаем директорию для заголовочного файла
mkdir -p "${HEADER_OUTPUT_DIR}"

# Генерируем заголовочный файл из проекта
if [ -f "cbindgen.toml" ]; then
    cbindgen --crate mobile-ios --output "${HEADER_FILE}" --config "cbindgen.toml" --lang c
else
    cbindgen --crate mobile-ios --output "${HEADER_FILE}" --lang c
fi

echo "✅ Done!"
echo "Universal library is in: ${ROOT_DIR}/target/ios/universal/release/metasecret-mobile.a"
echo "Header file is in: ${HEADER_FILE}"
