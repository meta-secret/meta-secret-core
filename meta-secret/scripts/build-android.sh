#!/bin/bash
set -e

# Установка целей для Android
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android

# Компиляция для Android (arm64-v8a)
echo "Компиляция для Android (arm64-v8a)..."
cargo build --target aarch64-linux-android --release --features mobile

# Компиляция для Android (armeabi-v7a)
echo "Компиляция для Android (armeabi-v7a)..."
cargo build --target armv7-linux-androideabi --release --features mobile

# Компиляция для Android (x86)
echo "Компиляция для Android (x86)..."
cargo build --target i686-linux-android --release --features mobile

# Компиляция для Android (x86_64)
echo "Компиляция для Android (x86_64)..."
cargo build --target x86_64-linux-android --release --features mobile

# Создание структуры директорий для Android библиотек
echo "Создание структуры директорий..."
mkdir -p target/android/libs/arm64-v8a
mkdir -p target/android/libs/armeabi-v7a
mkdir -p target/android/libs/x86
mkdir -p target/android/libs/x86_64

# Копирование библиотек
cp target/aarch64-linux-android/release/libmeta_secret_core.so target/android/libs/arm64-v8a/
cp target/armv7-linux-androideabi/release/libmeta_secret_core.so target/android/libs/armeabi-v7a/
cp target/i686-linux-android/release/libmeta_secret_core.so target/android/libs/x86/
cp target/x86_64-linux-android/release/libmeta_secret_core.so target/android/libs/x86_64/

echo "Готово! Библиотеки находятся в target/android/libs/"