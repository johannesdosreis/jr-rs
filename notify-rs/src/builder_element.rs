use std::marker::PhantomData;
use std::rc::Rc;

use crate::context::BuildContext;
use crate::element::{Element, ElementCore};
use crate::widget::{BuilderWidget, Widget};

pub(crate) struct BuilderElement<B> {
    core: ElementCore,
    _marker: PhantomData<fn() -> B>,
}

impl<B: BuilderWidget> BuilderElement<B> {
    pub(crate) fn new(widget: Rc<B>) -> Self {
        BuilderElement {
            core: ElementCore::new(widget),
            _marker: PhantomData,
        }
    }
}

impl<B: BuilderWidget> Element for BuilderElement<B> {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        vec![self.core.widget_as::<B>().build(ctx)]
    }
}

#[cfg(test)]
mod tests {
    use crate::app::App;
    use crate::context::BuildContext;
    use crate::test_support::{Leaf, Log, rc};
    use crate::widget::{BuilderWidget, Widget};
    use reactive_rs::Notifier;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicI32, Ordering::SeqCst};

    #[derive(Clone)]
    struct Counter {
        notifier: Arc<Notifier>,
        count: Arc<AtomicI32>,
        log: Log,
    }

    impl Counter {
        fn new(log: &Log) -> Counter {
            Counter {
                notifier: Arc::new(Notifier::new()),
                count: Arc::new(AtomicI32::new(0)),
                log: log.clone(),
            }
        }
    }

    impl BuilderWidget for Counter {
        fn notifier(&self) -> Option<&Arc<Notifier>> {
            Some(&self.notifier)
        }

        fn build(&self, _ctx: &mut BuildContext<'_>) -> Rc<dyn Widget> {
            self.log.push(format!("build {}", self.count.load(SeqCst)));
            rc(Leaf::new())
        }
    }

    struct Static;

    impl BuilderWidget for Static {
        fn build(&self, _ctx: &mut BuildContext<'_>) -> Rc<dyn Widget> {
            rc(Leaf::new())
        }
    }

    #[test]
    fn builder_widget_builds_its_child() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(rc(Counter::new(&log)));
        assert_eq!(app.debug_tree(), "Counter\n  Leaf\n");
        assert_eq!(log.take(), ["build 0"]);
    }

    #[test]
    fn set_changes_then_rebuilds_on_next_update() {
        let (log, mut app) = (Log::default(), App::new());
        let counter = Counter::new(&log);
        app.set_root(rc(counter.clone()));
        log.take();

        counter.set(|| {
            counter.count.fetch_add(1, SeqCst);
        });

        assert!(app.needs_update());
        app.update();
        assert_eq!(log.take(), ["build 1"]);
    }

    #[test]
    #[should_panic(expected = "BuilderWidget::set chamado em um widget sem notifier")]
    fn set_without_notifier_panics() {
        Static.set(|| {});
    }
}
