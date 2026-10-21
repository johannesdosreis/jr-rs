use std::marker::PhantomData;

/// Passed to [`Element::build`](crate::Element::build).
pub struct BuildContext<'a> {
    _marker: PhantomData<&'a mut ()>,
}

impl BuildContext<'_> {
    pub(crate) fn new() -> BuildContext<'static> {
        BuildContext {
            _marker: PhantomData,
        }
    }
}
