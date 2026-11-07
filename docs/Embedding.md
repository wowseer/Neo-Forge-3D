# Embedding in other projects

Neo Forge 3D consists of reusable building blocks that also work without the Godot table. With
them you can connect Forge's complete rules engine to your own front end (Rust, Godot, Bevy,
terminal, web …) or attach your own game table to a Forge host (NeoForge, Forge desktop, your own
Java programs).

```
 Front end (any)                    Protocol v3                     Forge (Java)
┌──────────────────────┐      JSON Lines, one message       ┌───────────────────────────────┐
│ Godot table (godot/) │      per line, UTF-8               │ forge-bridge                  │
│ or your own UI       │  ◀── request / state / event ───   │  clash.forgebridge            │
│                      │  ─── answer / start_match … ──▶    │   ClashBridge   (facade)      │
│ Rust:                │                                    │   TableLauncher (start table) │
│  clash-protocol      │   Transport:                       │  clash.forgebridge.engine     │
│  clash-forge-client  │   • stdin/stdout (own process)     │   (internal: Forge adapter)   │
└──────────────────────┘   • TCP 127.0.0.1:N (embedded)     │  clash.forgebridge.app        │
                                                            │   BridgeMain (own process)    │
                                                            └───────────────────────────────┘
                                                             Host integrations:
                                                             integrations/neoforge/
```

| Building block | Location | License | Dependencies | Purpose |
|---|---|---|---|---|
| **Protocol** | [`docs/Forge-Protocol.md`](Forge-Protocol.md) | – | – | Messages, commands, `Decision`/`Choice`, game state |
| **`clash-protocol`** (Rust crate) | `rust/crates/clash-protocol/` | GPL-3.0 | `serde_json` | Protocol model as Rust types, reading/writing the JSON form, commands, the `RulesEngine` interface |
| **`clash-forge-client`** (Rust crate) | `rust/crates/clash-forge-client/` | GPL-3.0 | `clash-protocol`, `serde_json` | Start the bridge as a Java process **or** connect to a host; poll messages without blocking |
| **Forge bridge** (Java) | `forge-bridge/` | GPL-3.0 | Forge jars (Forge 2.0.15+), Gson | Translates Forge's `PlayerController` calls into `Decision`s and back |
| **NeoForge integration** | `integrations/neoforge/` | GPL-3.0 | NeoForge, Forge bridge | Wrapper `ClashTable` + patch for NeoForge's classes |
| **Godot table** | `godot/`, `rust/src/` | GPL-3.0 | Godot 4.7, gdext, both crates | The 3D front end of this project |

Like Forge, the whole project is licensed under the **GPL-3.0** (`LICENSE`). Whoever embeds the
building blocks takes on this license. Forge itself (jar and card data) is not in the repository.

---

## 1. Your own front end in Rust (Forge as a separate process)

`Cargo.toml`:

```toml
[dependencies]
clash-protocol = { path = "…/neo-forge-3d/rust/crates/clash-protocol" }
clash-forge-client = { path = "…/neo-forge-3d/rust/crates/clash-forge-client" }
```

Requirements: Java 17+, a Forge installation (folder with `res/` and
`forge-gui-desktop-*-jar-with-dependencies.jar`) and the built bridge
(`forge-bridge/build.sh` → `forge-bridge/build/forge-bridge.jar`).

```rust
use clash_forge_client::{ForgeClient, Incoming, LaunchConfig};
use clash_protocol::{command, message_type, Action, Choice, Decision, Request, RulesEngine};

let mut client = ForgeClient::spawn(&LaunchConfig::new("/opt/forge", "/path/forge-bridge.jar"))?;
// every frame (or in a loop):
while let Some(msg) = client.poll() {
    match msg {
        Incoming::Message(m) => match message_type(&m) {
            "ready" => client.send(&command::StartMatch {
                decks: vec!["/abs/a.dck".into(), "/abs/b.dck".into()], // absolute paths
                ..Default::default()                                    // seat 0 = your UI
            }.to_json())?,
            "request" => {
                let req = Request::from_json(&m).unwrap();
                // show req.decision, let the player decide …
                if let Decision::Priority { .. } = req.decision {
                    client.submit(req.id, &Choice::Action(Action::Pass));
                }
            }
            "state" | "event" | "log" | "game_over" => { /* display */ }
            _ => {}
        },
        Incoming::Invalid(_) => {}
        Incoming::Closed => break,
    }
}
```

Runnable example (a table without graphics that plays one seat with a simple rule):

```sh
cd rust
cargo run -p clash-forge-client --example autoplay -- \
  ../forge-bridge/forge ../forge-bridge/build/forge-bridge.jar \
  ../forge-bridge/testdecks/rot.dck ../forge-bridge/testdecks/gruen.dck
```

Notes:
- Every `request` message must be answered with exactly one `answer`; Forge waits until then.
  `Request::labels` returns display texts for the entries of the decision, `Request::cards` the
  card data of the cards involved.
- The bridge sends the complete game state with every request (`state`) and on demand
  (`command::request_state`).
- Which Forge question becomes which `Decision` is described in
  [`Forge-Protocol.md`](Forge-Protocol.md), section "What the engine decides".

## 2. Your own front end in another language

The protocol is plain JSON Lines. Without the Rust crates:

```sh
java -Xmx2048m -Dfile.encoding=UTF-8 -Djava.awt.headless=true \
     -XX:+UseG1GC -XX:MinHeapFreeRatio=10 -XX:MaxHeapFreeRatio=40 -XX:G1PeriodicGCInterval=10000 \
     -cp forge-bridge.jar:forge-gui-desktop-…-jar-with-dependencies.jar \
     clash.forgebridge.app.BridgeMain          # working directory: the Forge folder
```

The `-XX` options are optional: they make the JVM return unused heap instead of keeping it
(Forge holds only about 200 MB of live data during a match; without them the bridge kept about
1.2 GB, with them about 0.7 GB). `clash-forge-client` passes them by itself (`JVM_MEMORY_ARGS`).

Then write lines to stdin (`{"cmd":"ping","nonce":1}`, `{"cmd":"start_match",…}`,
`{"cmd":"answer","id":…,"choice":{…}}`) and read lines from stdout. All other output from Forge
goes to stderr. `forge-bridge/run.sh` starts the bridge this way for trying it out by hand.

## 3. Attaching your own table to a Forge host (Java)

A host is a Java program that has already loaded Forge and starts matches itself (NeoForge, Forge
desktop, your own server). It includes the library `forge-bridge/build/forge-bridge-core.jar`
(no program entry point, needs only `forge-game`, `forge-ai`, `forge-core`, `forge-gui` and Gson)
and uses only two classes:

```java
import clash.forgebridge.ClashBridge;
import clash.forgebridge.TableLauncher;

// 1. Start the table (opens 127.0.0.1:N, starts "<godot> --path <project> -- --clash-port=N",
//    accepts the connection and attaches the bridge to it)
TableLauncher.Session table = TableLauncher.launch(new TableLauncher.Config(godotExe, projectDir));

// 2. Register the seat the table plays as a remote-controlled player
RegisteredPlayer human = new RegisteredPlayer(deck);   // Commander: RegisteredPlayer.forCommander(deck)
human.setPlayer(ClashBridge.remotePlayer("Player", 0));           // 0 = seat index

// 3. Start the match as usual (HostedMatch / Match); every decision of this seat goes to the table.
// 4. After the match
table.close();
```

- `ClashBridge.isRemote(lobbyPlayer)`: is a seat controlled by the table (e.g. so that an AI
  fallback of the host does not replace it)?
- If the player closes the table window in the middle of a match, their seats concede
  (`ClashBridge.concedeRemotePlayers`).
- Your own transport instead of `TableLauncher`: `ClashBridge.attach(outputStream, inputStream, onClose)`.
- A table other than Godot only has to understand `--clash-port=N`, connect to `127.0.0.1:N` and
  speak the protocol – in Rust: `ForgeClient::connect(port)` with
  `clash_forge_client::host_port(std::env::args())`. The `autoplay` example does exactly that
  when started with `--clash-port=N`.

Only `ClashBridge` and `TableLauncher` are public API. `clash.forgebridge.engine` is internal and
may change.

## 4. NeoForge

Ready-made integration in [`integrations/neoforge/`](../integrations/neoforge/README.md): the
wrapper `forge.neo.clash.ClashTable` (reads NeoForge's settings, decides which matches run on the
3D table – only 1 vs 1 –, calls `TableLauncher`) and a patch for four NeoForge classes and the
language files. Installing into a NeoForge repository:

```sh
git -C <neoforge> apply "<neo-forge-3d>/integrations/neoforge/neoforge-host.patch"
NEOFORGE=<neoforge> tools/sync_neoforge.sh
```

Or build NeoForge with the table in one step: `tools/neoforge_build.sh` (Maven, Forge checkout as
in NeoForge's `BUILDING.md`), then start it with `tools/neoforge_play.sh`.

## 5. Building

| What | Command | Result |
|---|---|---|
| Java bridge | `forge-bridge/build.sh` (Forge jar via `FORGE_JAR`, otherwise `forge-bridge/forge/` or `~/Games/Forge`) | `forge-bridge/build/forge-bridge-core.jar` (library), `forge-bridge/build/forge-bridge.jar` (program, main class `clash.forgebridge.app.BridgeMain`) |
| Rust crates + Godot extension | `cd rust && cargo build` | `rust/target/debug/libneo_forge_3d.so` |
| Test only the crates | `cd rust && cargo test -p clash-protocol -p clash-forge-client` | – |
| NeoForge with the 3D table (Maven) | `tools/neoforge_build.sh`, start with `tools/neoforge_play.sh` | `<forge>/forge-gui-neo/target/` |
| NeoForge patch jar (without Maven) | `tools/neoforge_patch.sh` | `forge-bridge/build/neoforge/neoforge-clash-patch.jar` |
| Regenerate the NeoForge patch | `tools/neoforge_export_patch.sh` | `integrations/neoforge/neoforge-host.patch` |
