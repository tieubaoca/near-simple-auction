cargo build --all --target wasm32-unknown-unknown --release
cp target/wasm32-unknown-unknown/release/near_demo.wasm ./res/contract.wasm