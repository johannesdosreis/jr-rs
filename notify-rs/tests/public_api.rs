//! Uses the crate the way another library would: only through its public API.

use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::thread;

use notify_rs::{
    App, BuildContext, BuilderWidget, Element, ElementCore, Listenable, Notifier, Widget,
};

/// A widget with its own element, which listens to an external notifier.
struct External {
    notifier: Arc<Notifier>,
    builds: Arc<AtomicUsize>,
}

impl Widget for External {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        let builds = self.builds.clone();
        Box::new(ExternalElement {
            core: ElementCore::new(self),
            builds,
            subscription: None,
        })
    }
}

struct ExternalElement {
    core: ElementCore,
    builds: Arc<AtomicUsize>,
    subscription: Option<(Arc<Notifier>, usize)>,
}

impl Element for ExternalElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        if self.subscription.is_none() {
            let notifier = self.core.widget_as::<External>().notifier.clone();
            let id = notifier.add_listener(self.core.dirty_listener());
            self.subscription = Some((notifier, id));
        }
        self.builds.fetch_add(1, SeqCst);
        Vec::new()
    }
}

impl Drop for ExternalElement {
    fn drop(&mut self) {
        if let Some((notifier, id)) = self.subscription.take() {
            notifier.remove_listener(id);
        }
    }
}

struct Other;

impl Widget for Other {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(OtherElement {
            core: ElementCore::new(self),
        })
    }
}

struct OtherElement {
    core: ElementCore,
}

impl Element for OtherElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        Vec::new()
    }
}

#[test]
fn custom_element_rebuilds_when_external_notifier_fires_from_another_thread() {
    let notifier = Arc::new(Notifier::new());
    let builds = Arc::new(AtomicUsize::new(0));
    let mut app = App::new();
    app.set_root(Rc::new(External {
        notifier: notifier.clone(),
        builds: builds.clone(),
    }));
    assert_eq!(builds.load(SeqCst), 1);

    let remote = notifier.clone();
    thread::spawn(move || remote.notify_listeners())
        .join()
        .unwrap();
    assert!(app.needs_update());
    app.update();
    assert_eq!(builds.load(SeqCst), 2);

    app.set_root(Rc::new(Other));
    notifier.notify_listeners();
    assert!(!app.needs_update());
}

#[derive(Clone)]
struct Clicks {
    notifier: Arc<Notifier>,
    count: Arc<AtomicUsize>,
    builds: Arc<AtomicUsize>,
}

impl BuilderWidget for Clicks {
    fn notifier(&self) -> Option<&Arc<Notifier>> {
        Some(&self.notifier)
    }

    fn build(&self, _ctx: &mut BuildContext<'_>) -> Rc<dyn Widget> {
        self.builds.fetch_add(1, SeqCst);
        Rc::new(Other)
    }
}

#[test]
fn builder_widget_works_from_outside_the_crate() {
    let clicks = Clicks {
        notifier: Arc::new(Notifier::new()),
        count: Arc::new(AtomicUsize::new(0)),
        builds: Arc::new(AtomicUsize::new(0)),
    };
    let mut app = App::new();
    app.set_root(Rc::new(clicks.clone()));
    assert_eq!(app.debug_tree(), "Clicks\n  Other\n");

    clicks.set(|| {
        clicks.count.fetch_add(1, SeqCst);
    });
    assert!(app.needs_update());
    app.update();

    assert_eq!(clicks.count.load(SeqCst), 1);
    assert_eq!(clicks.builds.load(SeqCst), 2);
}
