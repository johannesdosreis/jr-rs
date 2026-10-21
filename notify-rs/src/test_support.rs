// Not every helper is used by every module's tests.
#![allow(dead_code)]

use std::any::Any;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use reactive_rs::Notifier;

use crate::app::App;
use crate::context::BuildContext;
use crate::element::{Element, ElementCore};
use crate::geometry::{Constraints, Size};
use crate::key::Key;
use crate::widget::Widget;
use crate::widgets::SizedBox;

/// Records events, in order, across threads.
#[derive(Clone, Default)]
pub(crate) struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
    pub(crate) fn push(&self, entry: impl Into<String>) {
        self.0.lock().unwrap().push(entry.into());
    }

    pub(crate) fn take(&self) -> Vec<String> {
        std::mem::take(&mut self.0.lock().unwrap())
    }
}

pub(crate) fn rc<W: Widget>(widget: W) -> Rc<dyn Widget> {
    Rc::new(widget)
}

/// A widget without children.
pub(crate) struct Leaf {
    pub(crate) key: Option<Key>,
}

impl Leaf {
    pub(crate) fn new() -> Leaf {
        Leaf { key: None }
    }

    pub(crate) fn keyed(key: impl Into<Key>) -> Leaf {
        Leaf {
            key: Some(key.into()),
        }
    }
}

impl Widget for Leaf {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(LeafElement {
            core: ElementCore::new(self),
        })
    }
}

struct LeafElement {
    core: ElementCore,
}

impl Element for LeafElement {
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

/// A widget whose children are given directly.
pub(crate) struct Group {
    pub(crate) children: Vec<Rc<dyn Widget>>,
}

impl Group {
    pub(crate) fn new(children: Vec<Rc<dyn Widget>>) -> Group {
        Group { children }
    }
}

impl Widget for Group {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(GroupElement {
            core: ElementCore::new(self),
        })
    }
}

struct GroupElement {
    core: ElementCore,
}

impl Element for GroupElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        self.core.widget_as::<Group>().children.clone()
    }
}

/// A widget whose element logs "create {name}", "build {name}" and "drop {name}".
pub(crate) struct Probe {
    name: &'static str,
    log: Log,
    key: Option<Key>,
    children: Vec<Rc<dyn Widget>>,
    notifier: Option<Arc<Notifier>>,
}

impl Probe {
    pub(crate) fn new(name: &'static str, log: &Log) -> Probe {
        Probe {
            name,
            log: log.clone(),
            key: None,
            children: Vec::new(),
            notifier: None,
        }
    }

    pub(crate) fn with_key(mut self, key: impl Into<Key>) -> Probe {
        self.key = Some(key.into());
        self
    }

    pub(crate) fn with_children(mut self, children: Vec<Rc<dyn Widget>>) -> Probe {
        self.children = children;
        self
    }

    pub(crate) fn with_notifier(mut self, notifier: Arc<Notifier>) -> Probe {
        self.notifier = Some(notifier);
        self
    }
}

impl Widget for Probe {
    fn key(&self) -> Option<Key> {
        self.key.clone()
    }

    fn notifier(&self) -> Option<&Arc<Notifier>> {
        self.notifier.as_ref()
    }

    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        self.log.push(format!("create {}", self.name));
        let (name, log) = (self.name, self.log.clone());
        Box::new(ProbeElement {
            core: ElementCore::new(self),
            name,
            log,
        })
    }
}

struct ProbeElement {
    core: ElementCore,
    name: &'static str,
    log: Log,
}

impl Element for ProbeElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        self.log.push(format!("build {}", self.name));
        self.core.widget_as::<Probe>().children.clone()
    }
}

impl Drop for ProbeElement {
    fn drop(&mut self) {
        self.log.push(format!("drop {}", self.name));
    }
}

/// A widget whose element logs "layout {name}" on every layout, lays out its
/// children with configurable constraints and returns a configurable size.
pub(crate) struct LayoutProbe {
    name: &'static str,
    log: Log,
    size: Size,
    children: Vec<Rc<dyn Widget>>,
    child_constraints: Option<Constraints>,
    parent_uses_size: bool,
    sized_by_parent: bool,
    notifier: Option<Arc<Notifier>>,
    parent_data: Option<u32>,
    raw_size: bool,
}

impl LayoutProbe {
    pub(crate) fn new(name: &'static str, log: &Log) -> LayoutProbe {
        LayoutProbe {
            name,
            log: log.clone(),
            size: Size::ZERO,
            children: Vec::new(),
            child_constraints: None,
            parent_uses_size: true,
            sized_by_parent: false,
            notifier: None,
            parent_data: None,
            raw_size: false,
        }
    }

    pub(crate) fn size(mut self, size: Size) -> LayoutProbe {
        self.size = size;
        self
    }

    pub(crate) fn children(mut self, children: Vec<Rc<dyn Widget>>) -> LayoutProbe {
        self.children = children;
        self
    }

    pub(crate) fn child_constraints(mut self, constraints: Constraints) -> LayoutProbe {
        self.child_constraints = Some(constraints);
        self
    }

    pub(crate) fn parent_uses_size(mut self, uses: bool) -> LayoutProbe {
        self.parent_uses_size = uses;
        self
    }

    pub(crate) fn sized_by_parent(mut self, sized: bool) -> LayoutProbe {
        self.sized_by_parent = sized;
        self
    }

    pub(crate) fn notifier(mut self, notifier: Arc<Notifier>) -> LayoutProbe {
        self.notifier = Some(notifier);
        self
    }

    pub(crate) fn parent_data(mut self, data: u32) -> LayoutProbe {
        self.parent_data = Some(data);
        self
    }

    /// Returns the configured size without passing it through the constraints.
    pub(crate) fn raw_size(mut self) -> LayoutProbe {
        self.raw_size = true;
        self
    }
}

impl Widget for LayoutProbe {
    fn notifier(&self) -> Option<&Arc<Notifier>> {
        self.notifier.as_ref()
    }

    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(LayoutProbeElement {
            core: ElementCore::new(self),
        })
    }
}

struct LayoutProbeElement {
    core: ElementCore,
}

impl Element for LayoutProbeElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        self.core.widget_as::<LayoutProbe>().children.clone()
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let widget = self.core.widget_as::<LayoutProbe>();
        widget.log.push(format!("layout {}", widget.name));
        let child_constraints = widget.child_constraints.unwrap_or(constraints);
        for index in 0..self.core.children().len() {
            self.core
                .layout_child(index, child_constraints, widget.parent_uses_size);
        }
        if widget.raw_size {
            widget.size
        } else {
            constraints.constrain(widget.size)
        }
    }

    fn sized_by_parent(&self) -> bool {
        self.core.widget_as::<LayoutProbe>().sized_by_parent
    }

    fn parent_data(&self) -> Option<&dyn Any> {
        let widget: &dyn Any = &**self.core.widget();
        widget
            .downcast_ref::<LayoutProbe>()?
            .parent_data
            .as_ref()
            .map(|data| data as &dyn Any)
    }
}

/// Lays `root` out in a window (or volume) of `size` and returns `debug_layout`.
pub(crate) fn layout_of(size: Size, root: Rc<dyn Widget>) -> String {
    app_with(size, root).debug_layout()
}

/// An App laid out in a window (or volume) of `size`, with `root` mounted.
pub(crate) fn app_with(size: Size, root: Rc<dyn Widget>) -> App {
    let mut app = App::new();
    app.set_size(size);
    app.set_root(root);
    app
}

pub(crate) fn root_children(app: &App) -> &[Box<dyn Element>] {
    app.root.as_ref().unwrap().children()
}

/// `debug_layout` of `child` given unbounded constraints on every axis.
pub(crate) fn unbounded_layout(size: Size, child: Rc<dyn Widget>) -> String {
    let log = Log::default();
    layout_of(
        size,
        rc(LayoutProbe::new("root", &log)
            .child_constraints(Constraints::unbounded())
            .children(vec![child])),
    )
}

/// A `SizedBox` with the given width and height.
pub(crate) fn sized(width: f32, height: f32) -> Rc<dyn Widget> {
    rc(SizedBox::new().width(width).height(height))
}
