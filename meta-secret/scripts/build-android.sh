#!/bin/bash
set -e

# Установка целей для Android
rustup target add aarch64-linux-android

# Компиляция для Android (arm64-v8a)
echo "Компиляция для Android (arm64-v8a)..."
cargo build --package meta-secret-core --target aarch64-linux-android --release --features mobile

# Создание структуры директорий для Android библиотек
echo "Создание структуры директорий..."
mkdir -p target/android/libs/arm64-v8a

# Копирование библиотек
cp target/aarch64-linux-android/release/libmeta_secret_core.so target/android/libs/arm64-v8a/

echo "Готово! Библиотека для arm64-v8a находится в target/android/libs/arm64-v8a/"