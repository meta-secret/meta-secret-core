#!/bin/bash
set -e

# Переходим в корневую директорию проекта meta-secret
cd "$(dirname "$0")/../../../.."

# Собираем библиотеку
echo "Собираем библиотеку..."
cargo build --release -p uniffi-mobile

# Создаем директории для биндингов
mkdir -p target/swift
mkdir -p target/kotlin

# Генерируем Swift биндинги
echo "Генерируем Swift биндинги..."
cargo run -p uniffi-bindgen -- generate \
  --library target/release/libmeta_core_mobile.dylib \
  --language swift \
  --out-dir target/swift

# Генерируем Kotlin биндинги
echo "Генерируем Kotlin биндинги..."
cargo run -p uniffi-bindgen -- generate \
  --library target/release/libmeta_core_mobile.dylib \
  --language kotlin \
  --out-dir target/kotlin

echo "Биндинги успешно сгенерированы:"
echo "- Swift: target/swift"
echo "- Kotlin: target/kotlin" 