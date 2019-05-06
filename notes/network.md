[seq, ack, [LEN, payload]]
seq: u32,
ack: u32,
LEN: varint,

the idea is that you've got a circular buffer of outgoing messages.
if you buffer becomes FULL, the connection reckons that your peer has lost connection.
the START of the sequence moves forward only with ACKS.

