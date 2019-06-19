use crate::LocId;

#[derive(Debug, Clone, Copy)]
pub struct Proto<'a>(pub &'a [Rule<'a>]);

#[derive(Debug, Clone, Copy)]
pub struct Rule<'a> {
    pub pred: Pred<'a>,
    pub actions: &'a [Action<'a>],
}

#[derive(Debug, Clone, Copy)]
pub struct Action<'a> {
    pub putter: LocId,
    pub getters: &'a [LocId],
}

#[derive(Debug, Clone, Copy)]
pub enum Pred<'a> {
    True,
    None(&'a [Pred<'a>]),
    And(&'a [Pred<'a>]),
    Or(&'a [Pred<'a>]),
    Eq(LocId, LocId),
}

pub const PROTO: Proto<'static> = Proto(&[Rule {
    pred: Pred::True,
    actions: &[Action {
        putter: 0,
        getters: &[0, 1, 2],
    }],
}]);
