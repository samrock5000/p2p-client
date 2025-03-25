# # 🌟 Light P2P BCH Client Example 🌟

## 🚀 Clone the Repo
```bash
git clone https://github.com/samrock5000/p2p-client
```

## ▶️ Run the Client

```bash
cargo run -- --network chipnet --debug
```

## ⚠️ Caution

###### Creates client data in $HOME/.nakamoto by default



_Update **root** to change directory path:_  
```rust
    let cfg = Config {
        network,
        connect,
        root: PathBuf::from(env::var(HOME_DIR).unwrap_or_default()),
        listen: vec![], // Don't listen for incoming connections.
        ..Config::default()
    };

```

