threadless without mem is simple.

threads coordinate by updating POINTERS in the shared structure
PUTTERS can update their pointer and then fall asleep.
putter needs to know 2 things after it wakes up:
1. when are all getters done reading
2. has the memory been MOVED (yes/no)

GETTERS need to know:
1. when is it safe to read
2. is it allowed to move the data (yes/no)
3. which datum / putter to get from.

---------------------
memory complicates matters. the problem becomes, first and foremost:
1. there is no longer a putter thread: no centralized authority on the datum

---------------------
we need to ELECT a leader for the memory-get operation
this getter is responsible for:
