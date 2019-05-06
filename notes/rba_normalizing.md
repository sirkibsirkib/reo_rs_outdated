currently I run into problems where several rules LOOK like they can be reduced but this isn't obvious at a glance.

EG:
rules: [
    XXF =1=> XXT,
    TTT =1=> FFF,
    XFT =1=> XTT,
    FTT =1=> TFT
]


Intuitively, I should be able to rewrite this to
rules: [
    XXX =1=> XXX
]

what's stopping me?

