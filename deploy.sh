#!/bin/bash

# Build the project
cargo build --release --target wasm32-unknown-unknown --package rust-token-backend

# Check if the build was successful
if [ $? -ne 0 ]; then
    echo "Build failed. Exiting."
    exit 1
fi

# Extract candid interface from the WASM file
candid-extractor target/wasm32-unknown-unknown/release/rust_token_backend.wasm > src/rust-token-backend/rust-token-backend.did

# Check if candid extraction was successful
if [ $? -ne 0 ]; then
    echo "Candid extraction failed. Exiting."
    exit 1
fi

dfx deploy

echo "Deployment script completed successfully."