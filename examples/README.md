
# Nakamoto-Cash Watch-Only Client Example 🚀

![til](https://github.com/samrock5000/p2p-client/examples/assets/watch-demo.gif)

This is an example application demonstrating a watch-only client for Bitcoin Cash using the `nakamoto-cash` crate (a fork of the Nakamoto P2P client). The application provides a basic UI for monitoring transactions matching specific criteria using Bloom filters. 🔍

## Features ✨
- Connects to Bitcoin Cash P2P network (supports chipnet testnet) 🌐
- Implements a watch-only client with Bloom filter support 🕵️‍♂️
- Basic Slint-based UI for interaction 🖥️
- Displays matched transactions and connected peers 📊
- Clipboard functionality for copying transaction IDs 📋
- Debug logging support 🐞

## Prerequisites ✅
- Rust (stable) and Cargo 🦀
- Git 🌿
- Basic command-line knowledge ⌨️

## Installation 🛠️

1. Clone the repository:
```bash
git clone https://github.com/samrock5000/p2p-client
cd p2p-client
```
2. Navigate to the examples directory:
```bash
cd examples
```
## Running the Application
Basic Usage
Run the client on the chipnet testnet with debug logging:
```bash
cargo run -- --network chipnet --debug`
```

## Command Line Options
--network: Specify the network (default: chipnet)

--connect: Connect to specific node(s) (e.g., 127.0.0.1:8333)

--debug: Enable debug logging (optional)

Example with a specific node:
```bash
cargo run -- --network chipnet --connect 127.0.0.1:48333 --debug
```

### Usage 

1. Launch the application using one of the run commands above 

2. The UI will display:
- Current block height 

- Connected peers supporting Bloom filters 

- Matched transactions 

3. Add items to watch via the Bloom filter through the UI 

4. Copy transaction IDs to clipboard as needed 



