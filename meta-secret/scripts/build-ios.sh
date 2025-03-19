#!/bin/bash
set -e

# Установка целей для iOS
rustup target add aarch64-apple-ios x86_64-apple-ios

# Компиляция для iOS device (arm64)
echo "Компиляция для iOS устройств (arm64)..."
cargo build --target aarch64-apple-ios --release --features mobile

# Компиляция для iOS симулятора (x86_64)
echo "Компиляция для iOS симулятора (x86_64)..."
cargo build --target x86_64-apple-ios --release --features mobile

# Создание универсальной бинарной библиотеки
echo "Создание универсальной бинарной библиотеки..."
mkdir -p target/universal/release
lipo -create \
  target/aarch64-apple-ios/release/libmeta_secret_core.a \
  target/x86_64-apple-ios/release/libmeta_secret_core.a \
  -output target/universal/release/libmeta_secret_core.a

echo "Готово! Библиотека находится в target/universal/release/libmeta_secret_core.a"