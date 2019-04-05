# Idea
This is something Sung mentioned and it's a nice idea.
So you control the environment of the protocol.
You generate an API for each atomic component. The API is constructed as a local-view of some global state
machine of the protocol representing all transitions that are valid moves.
The idea is that we generate an API for atomic components using constant TOKEN TYPES that
dictate what happens.  


```Rust

pub struct Token


```