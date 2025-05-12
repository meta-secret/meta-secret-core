#!/bin/bash
set -e

# Переходим в корневую директорию проекта meta-secret
cd "$(dirname "$0")/../../../.."

# Собираем библиотеку
cargo build --release -p uniffi-mobile

# Создаем директорию для Swift биндингов
mkdir -p target/swift

# Генерируем Swift биндинги
cargo run -p uniffi-bindgen -- generate \
  --library target/release/libmeta_core_mobile.dylib \
  --language swift \
  --out-dir target/swift

echo "Swift биндинги успешно сгенерированы в директории target/swift" 