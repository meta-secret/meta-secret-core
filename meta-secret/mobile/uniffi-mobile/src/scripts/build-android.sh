#!/bin/bash
set -e

# Переходим в корневую директорию проекта meta-secret
cd "$(dirname "$0")/../../../.."

# Собираем библиотеку
cargo build --release -p uniffi-mobile

# Создаем директорию для Kotlin биндингов
mkdir -p target/kotlin

# Генерируем Kotlin биндинги
cargo run -p uniffi-bindgen -- generate \
  --library target/release/libmeta_core_mobile.dylib \
  --language kotlin \
  --out-dir target/kotlin

echo "Kotlin биндинги успешно сгенерированы в директории target/kotlin" 