# Battleship+ (Implementation Group A)

Battleship+ written in Rust🦀

## Battleship+
Multiplayer [Battleship](https://en.wikipedia.org/wiki/Battleship_(game)) game with some extra features like ship movement, special attacks. \
Based on this [RFC](https://github.com/FloBoJa/Battleship-Plus/files/10885949/rfc.pdf). \
Using [QUIC](https://en.wikipedia.org/wiki/QUIC) and [Protocol Buffers](https://en.wikipedia.org/wiki/Protocol_Buffers) for the networking.



## Usage
### Client:
`cargo run --package battleship_plus_client --bin battleship_plus_client` (`--feature wayland` for Wayland support)
### Server:
`cargo run --package battleship_plus_server --bin battleship_plus_server`


## Used Libraries
* [bevy](https://github.com/bevyengine/bevy) (game engine)
* [quinn](https://github.com/quinn-rs/quinn) (QUIC implementation)
* [prost](https://github.com/tokio-rs/prost) (Protocol Buffers implementation)
* [tokio](https://github.com/tokio-rs/tokio) (async handling)

## Issues found during the final InterOp test:

**Client:**
* Sometimes crashes under unknown conditions.

**Server:**
* Some certificate problems with [msquick](https://github.com/microsoft/msquic) (TLS)
* Send own and team Ships in VisionEvent (not exactly defined in RFC)
* (Some) events are visible to hostiles out of range
