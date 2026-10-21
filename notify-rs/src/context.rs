use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Weak};

use crate::element::ElementCore;
use crate::flags::Flags;

/// The elements that read a provided value, keyed by the address of their flags.
pub(crate) type Registry = Rc<RefCell<HashMap<usize, Weak<Flags>>>>;

type EqFn = fn(&dyn Any, &dyn Any) -> bool;

/// The values an element read with `get` in its last build (`None`: nothing was provided).
pub(crate) type Dependencies = HashMap<TypeId, Option<Rc<dyn Any>>>;

/// A value visible to the descendants of the element that provided it.
#[derive(Clone)]
pub(crate) struct ScopeEntry {
    pub(crate) value: Rc<dyn Any>,
    pub(crate) dependents: Registry,
}

/// The values provided by the ancestors of the element being visited.
pub(crate) type Scope = Rc<HashMap<TypeId, ScopeEntry>>;

/// A value passed to `provide` during the current build.
pub(crate) struct NewValue {
    pub(crate) value: Rc<dyn Any>,
    pub(crate) eq: EqFn,
}

/// A value an element provides, kept between builds.
pub(crate) struct Provided {
    pub(crate) value: Rc<dyn Any>,
    pub(crate) eq: EqFn,
    pub(crate) dependents: Registry,
}

/// Passed to [`Element::build`](crate::Element::build). Shares values with
/// descendants through [`provide`](Self::provide) and reads values shared by
/// ancestors through [`get`](Self::get).
pub struct BuildContext<'a> {
    scope: &'a Scope,
    flags: &'a Arc<Flags>,
    provided: HashMap<TypeId, NewValue>,
    dependencies: Dependencies,
}

impl<'a> BuildContext<'a> {
    pub(crate) fn new(scope: &'a Scope, flags: &'a Arc<Flags>) -> Self {
        BuildContext {
            scope,
            flags,
            provided: HashMap::new(),
            dependencies: HashMap::new(),
        }
    }

    /// Makes `value` visible to the descendants of the element being built.
    /// Dependents are rebuilt only when it differs from the previous value.
    pub fn provide<T: PartialEq + 'static>(&mut self, value: T) {
        let value = NewValue {
            value: Rc::new(value),
            eq: eq_as::<T>,
        };
        self.provided.insert(TypeId::of::<T>(), value);
    }

    /// The `T` provided by the nearest ancestor, if any. The element being built
    /// is rebuilt when that value changes.
    pub fn get<T: 'static>(&mut self) -> Option<Rc<T>> {
        let entry = self.scope.get(&TypeId::of::<T>());
        self.dependencies
            .insert(TypeId::of::<T>(), entry.map(|entry| entry.value.clone()));
        let entry = entry?;
        entry
            .dependents
            .borrow_mut()
            .insert(Arc::as_ptr(self.flags) as usize, Arc::downgrade(self.flags));
        entry.value.clone().downcast::<T>().ok()
    }

    pub(crate) fn finish(self) -> (HashMap<TypeId, NewValue>, Dependencies) {
        (self.provided, self.dependencies)
    }
}

fn eq_as<T: PartialEq + 'static>(a: &dyn Any, b: &dyn Any) -> bool {
    a.downcast_ref::<T>() == b.downcast_ref::<T>()
}

/// Stores what an element provided in its last build. Dependents of changed or
/// removed values are marked; returns whether a type was added, in which case
/// the whole subtree must check its dependencies.
pub(crate) fn apply_provided(core: &mut ElementCore, new: HashMap<TypeId, NewValue>) -> bool {
    let mut old = std::mem::take(&mut core.provided);
    let mut added = false;
    for (type_id, NewValue { value, eq }) in new {
        let provided = match old.remove(&type_id) {
            Some(previous) if (previous.eq)(&*previous.value, &*value) => previous,
            Some(previous) => {
                mark_dependents(&previous.dependents, &core.flags);
                previous.dependents.borrow_mut().clear();
                Provided {
                    value,
                    eq,
                    dependents: previous.dependents,
                }
            }
            None => {
                added = true;
                Provided {
                    value,
                    eq,
                    dependents: Registry::default(),
                }
            }
        };
        core.provided.insert(type_id, provided);
    }
    for removed in old.into_values() {
        mark_dependents(&removed.dependents, &core.flags);
    }
    added
}

fn mark_dependents(dependents: &Registry, provider: &Arc<Flags>) {
    for dependent in dependents.borrow().values() {
        if let Some(flags) = dependent.upgrade() {
            flags.mark_within(provider);
        }
    }
}

/// The scope seen by the children of `core`'s element.
pub(crate) fn child_scope(parent: &Scope, core: &ElementCore) -> Scope {
    if core.provided.is_empty() {
        return parent.clone();
    }
    let mut scope = (**parent).clone();
    for (type_id, provided) in &core.provided {
        let entry = ScopeEntry {
            value: provided.value.clone(),
            dependents: provided.dependents.clone(),
        };
        scope.insert(*type_id, entry);
    }
    Rc::new(scope)
}

/// Whether a value the element read in its last build is no longer the one in `scope`.
pub(crate) fn dependencies_changed(core: &ElementCore, scope: &Scope) -> bool {
    core.dependencies.iter().any(|(type_id, seen)| {
        let current = scope.get(type_id).map(|entry| &entry.value);
        match (seen, current) {
            (Some(seen), Some(current)) => !Rc::ptr_eq(seen, current),
            (None, None) => false,
            _ => true,
        }
    })
}

#[cfg(test)]
mod tests {
    use crate::app::App;
    use crate::context::BuildContext;
    use crate::element::{Element, ElementCore};
    use crate::test_support::{Group, Log, Probe, rc};
    use crate::widget::Widget;
    use reactive_rs::Notifier;
    use std::any::TypeId;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

    #[derive(Debug, PartialEq)]
    struct Theme(i32);

    struct Provider {
        theme: Option<i32>,
        child: Rc<dyn Widget>,
    }

    fn provider(theme: Option<i32>, child: &Rc<dyn Widget>) -> Rc<dyn Widget> {
        rc(Provider {
            theme,
            child: child.clone(),
        })
    }

    impl Widget for Provider {
        fn create_element(self: Rc<Self>) -> Box<dyn Element> {
            Box::new(ProviderElement {
                core: ElementCore::new(self),
            })
        }
    }

    struct ProviderElement {
        core: ElementCore,
    }

    impl Element for ProviderElement {
        fn core(&self) -> &ElementCore {
            &self.core
        }

        fn core_mut(&mut self) -> &mut ElementCore {
            &mut self.core
        }

        fn build(&mut self, ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
            let widget = self.core.widget_as::<Provider>();
            if let Some(value) = widget.theme {
                ctx.provide(Theme(value));
            }
            vec![widget.child.clone()]
        }
    }

    /// Logs "{name}={theme}" or "{name}=none" on every build, or "{name} sem ler"
    /// when `reads` is off.
    struct Reader {
        name: &'static str,
        log: Log,
        reads: Arc<AtomicBool>,
        notifier: Option<Arc<Notifier>>,
    }

    impl Reader {
        fn new(name: &'static str, log: &Log) -> Reader {
            Reader {
                name,
                log: log.clone(),
                reads: Arc::new(AtomicBool::new(true)),
                notifier: None,
            }
        }
    }

    impl Widget for Reader {
        fn notifier(&self) -> Option<&Arc<Notifier>> {
            self.notifier.as_ref()
        }

        fn create_element(self: Rc<Self>) -> Box<dyn Element> {
            Box::new(ReaderElement {
                core: ElementCore::new(self),
            })
        }
    }

    struct ReaderElement {
        core: ElementCore,
    }

    impl Element for ReaderElement {
        fn core(&self) -> &ElementCore {
            &self.core
        }

        fn core_mut(&mut self) -> &mut ElementCore {
            &mut self.core
        }

        fn build(&mut self, ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
            let widget = self.core.widget_as::<Reader>();
            if widget.reads.load(SeqCst) {
                let theme = ctx.get::<Theme>();
                let value = theme.map_or("none".to_string(), |theme| theme.0.to_string());
                widget.log.push(format!("{}={value}", widget.name));
            } else {
                widget.log.push(format!("{} sem ler", widget.name));
            }
            Vec::new()
        }
    }

    struct SelfReader {
        log: Log,
    }

    impl Widget for SelfReader {
        fn create_element(self: Rc<Self>) -> Box<dyn Element> {
            Box::new(SelfReaderElement {
                core: ElementCore::new(self),
            })
        }
    }

    struct SelfReaderElement {
        core: ElementCore,
    }

    impl Element for SelfReaderElement {
        fn core(&self) -> &ElementCore {
            &self.core
        }

        fn core_mut(&mut self) -> &mut ElementCore {
            &mut self.core
        }

        fn build(&mut self, ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
            ctx.provide(Theme(5));
            let seen = ctx
                .get::<Theme>()
                .map_or("none".to_string(), |t| t.0.to_string());
            self.core
                .widget_as::<SelfReader>()
                .log
                .push(format!("self={seen}"));
            Vec::new()
        }
    }

    #[test]
    fn get_finds_the_nearest_provider() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(provider(
            Some(1),
            &provider(Some(2), &rc(Reader::new("r", &log))),
        ));
        assert_eq!(log.take(), ["r=2"]);
    }

    #[test]
    fn element_does_not_see_its_own_provide() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(rc(SelfReader { log: log.clone() }));
        assert_eq!(log.take(), ["self=none"]);
    }

    #[test]
    fn equal_value_rebuilds_nobody() {
        let (log, mut app) = (Log::default(), App::new());
        let child = rc(Group::new(vec![rc(Reader::new("r", &log))]));
        app.set_root(provider(Some(1), &child));
        log.take();
        app.set_root(provider(Some(1), &child));
        assert!(log.take().is_empty());
    }

    #[test]
    fn changed_value_rebuilds_only_dependents() {
        let (log, mut app) = (Log::default(), App::new());
        let child = rc(Group::new(vec![
            rc(Reader::new("r", &log)),
            rc(Probe::new("p", &log)),
        ]));
        app.set_root(provider(Some(1), &child));
        log.take();
        app.set_root(provider(Some(2), &child));
        assert_eq!(log.take(), ["r=2"]);
    }

    #[test]
    fn removed_value_falls_back_to_the_outer_provider() {
        let (log, mut app) = (Log::default(), App::new());
        let child = rc(Group::new(vec![rc(Reader::new("r", &log))]));
        app.set_root(provider(Some(1), &provider(Some(2), &child)));
        log.take();
        app.set_root(provider(Some(1), &provider(None, &child)));
        assert_eq!(log.take(), ["r=1"]);
    }

    #[test]
    fn added_value_rebuilds_readers_below() {
        let (log, mut app) = (Log::default(), App::new());
        let child = rc(Group::new(vec![rc(Reader::new("r", &log))]));
        app.set_root(provider(Some(1), &provider(None, &child)));
        assert_eq!(log.take(), ["r=1"]);
        app.set_root(provider(Some(1), &provider(Some(3), &child)));
        assert_eq!(log.take(), ["r=3"]);

        let mut app = App::new();
        let child = rc(Group::new(vec![rc(Reader::new("u", &log))]));
        app.set_root(provider(None, &child));
        assert_eq!(log.take(), ["u=none"]);
        app.set_root(provider(Some(3), &child));
        assert_eq!(log.take(), ["u=3"]);
    }

    #[test]
    fn dependent_registry_does_not_grow() {
        let (log, mut app) = (Log::default(), App::new());
        let notifier = Arc::new(Notifier::new());
        let mut reader = Reader::new("r", &log);
        reader.notifier = Some(notifier.clone());
        app.set_root(provider(Some(1), &rc(reader)));
        for _ in 0..100 {
            notifier.notify_listeners();
            app.update();
        }
        let root = app.root.as_ref().unwrap();
        let provided = &root.core().provided[&TypeId::of::<Theme>()];
        assert_eq!(provided.dependents.borrow().len(), 1);
    }

    #[test]
    fn reader_that_stops_reading_is_not_rebuilt_after_registry_clears() {
        let (log, mut app) = (Log::default(), App::new());
        let notifier = Arc::new(Notifier::new());
        let mut reader = Reader::new("r", &log);
        reader.notifier = Some(notifier.clone());
        let reads = reader.reads.clone();
        let child = rc(reader);
        app.set_root(provider(Some(1), &child));
        assert_eq!(log.take(), ["r=1"]);

        reads.store(false, SeqCst);
        notifier.notify_listeners();
        app.update();
        assert_eq!(log.take(), ["r sem ler"]);

        app.set_root(provider(Some(2), &child));
        assert!(log.take().len() <= 1);
        app.set_root(provider(Some(3), &child));
        assert!(log.take().is_empty());
    }
}
