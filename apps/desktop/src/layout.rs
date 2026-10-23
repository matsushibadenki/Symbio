//! Persistent, bounded dock tree. Window/WebView instances own no layout state.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Panel {
    Objects,
    Inspector,
    Notebook,
}
impl Panel {
    pub const ALL: [Self; 3] = [Self::Objects, Self::Inspector, Self::Notebook];
    pub fn label(self) -> &'static str {
        match self {
            Self::Objects => "objects",
            Self::Inspector => "inspector",
            Self::Notebook => "notebook",
        }
    }
    pub fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.label() == s)
    }
    pub fn index(self) -> usize {
        match self {
            Self::Objects => 0,
            Self::Inspector => 1,
            Self::Notebook => 2,
        }
    }
    pub fn window(self) -> &'static str {
        match self {
            Self::Objects => "float-objects",
            Self::Inspector => "float-inspector",
            Self::Notebook => "float-notebook",
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Horizontal,
    Vertical,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Zone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
    Floating,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Node {
    Tabs {
        id: u32,
        panels: Vec<Panel>,
        active: Panel,
    },
    Split {
        id: u32,
        axis: Axis,
        ratio: f64,
        first: Box<Node>,
        second: Box<Node>,
    },
}
impl Node {
    fn contains(&self, p: Panel) -> bool {
        match self {
            Self::Tabs { panels, .. } => panels.contains(&p),
            Self::Split { first, second, .. } => first.contains(p) || second.contains(p),
        }
    }
    pub fn first_panel(&self) -> Panel {
        match self {
            Self::Tabs { active, .. } => *active,
            Self::Split { first, .. } => first.first_panel(),
        }
    }
    pub fn tabs(&self, p: Panel) -> Option<&[Panel]> {
        match self {
            Self::Tabs { panels, .. } => panels.contains(&p).then_some(panels),
            Self::Split { first, second, .. } => first.tabs(p).or_else(|| second.tabs(p)),
        }
    }
    fn remove(self, p: Panel) -> Option<Self> {
        match self {
            Self::Tabs {
                id,
                mut panels,
                mut active,
            } => {
                panels.retain(|v| *v != p);
                if panels.is_empty() {
                    None
                } else {
                    if active == p {
                        active = panels[0];
                    }
                    Some(Self::Tabs { id, panels, active })
                }
            }
            Self::Split {
                id,
                axis,
                ratio,
                first,
                second,
            } => match (first.remove(p), second.remove(p)) {
                (Some(a), Some(b)) => Some(Self::Split {
                    id,
                    axis,
                    ratio,
                    first: Box::new(a),
                    second: Box::new(b),
                }),
                (a, b) => a.or(b),
            },
        }
    }
    fn select(&mut self, p: Panel) {
        match self {
            Self::Tabs { panels, active, .. } => {
                if panels.contains(&p) {
                    *active = p;
                }
            }
            Self::Split { first, second, .. } => {
                first.select(p);
                second.select(p);
            }
        }
    }
    pub fn set_ratio(&mut self, target: u32, value: f64) -> bool {
        match self {
            Self::Tabs { .. } => false,
            Self::Split {
                id,
                ratio,
                first,
                second,
                ..
            } => {
                if *id == target {
                    *ratio = value;
                    true
                } else {
                    first.set_ratio(target, value) || second.set_ratio(target, value)
                }
            }
        }
    }
    fn insert(
        &mut self,
        p: Panel,
        target: Panel,
        zone: Zone,
        next: &mut u32,
    ) -> Result<(), &'static str> {
        match self {
            Self::Tabs { panels, active, .. } if panels.contains(&target) => {
                if zone == Zone::Center {
                    let i = panels.iter().position(|p| *p == target).unwrap() + 1;
                    panels.insert(i, p);
                    *active = p;
                } else {
                    let leaf = leaf(alloc(next)?, p);
                    let id = alloc(next)?;
                    let old = self.clone();
                    let before = matches!(zone, Zone::Left | Zone::Top);
                    *self = Self::Split {
                        id,
                        axis: if matches!(zone, Zone::Left | Zone::Right) {
                            Axis::Horizontal
                        } else {
                            Axis::Vertical
                        },
                        ratio: 0.5,
                        first: Box::new(if before { leaf.clone() } else { old.clone() }),
                        second: Box::new(if before { old } else { leaf }),
                    };
                }
                Ok(())
            }
            Self::Split { first, second, .. } => {
                if first.contains(target) {
                    first.insert(p, target, zone, next)
                } else {
                    second.insert(p, target, zone, next)
                }
            }
            _ => Err("invalid_layout"),
        }
    }
    fn return_slot(&self, p: Panel) -> Option<Destination> {
        match self {
            Self::Tabs { panels, .. } => panels.iter().find(|v| **v != p).map(|v| Destination {
                target: Some(*v),
                zone: Zone::Center,
            }),
            Self::Split {
                axis,
                first,
                second,
                ..
            } => {
                let (branch, sibling, zone) = if first.contains(p) {
                    (
                        first,
                        second,
                        if *axis == Axis::Horizontal {
                            Zone::Left
                        } else {
                            Zone::Top
                        },
                    )
                } else {
                    (
                        second,
                        first,
                        if *axis == Axis::Horizontal {
                            Zone::Right
                        } else {
                            Zone::Bottom
                        },
                    )
                };
                branch.return_slot(p).or_else(|| {
                    Some(Destination {
                        target: Some(sibling.first_panel()),
                        zone,
                    })
                })
            }
        }
    }
}
fn active_tab(node: &Node, p: Panel) -> Option<Panel> {
    match node {
        Node::Tabs { panels, active, .. } => panels.contains(&p).then_some(*active),
        Node::Split { first, second, .. } => active_tab(first, p).or_else(|| active_tab(second, p)),
    }
}
fn alloc(next: &mut u32) -> Result<u32, &'static str> {
    let id = *next;
    *next = next.checked_add(1).ok_or("invalid_layout")?;
    Ok(id)
}
fn leaf(id: u32, p: Panel) -> Node {
    Node::Tabs {
        id,
        panels: vec![p],
        active: p,
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Destination {
    pub target: Option<Panel>,
    pub zone: Zone,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FloatingGroup {
    pub host: Panel,
    pub panels: Vec<Panel>,
    pub active: Panel,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LayoutState {
    pub version: u32,
    #[serde(default)]
    pub revision: u64,
    pub next_id: u32,
    pub root: Option<Node>,
    pub floating: Vec<Panel>,
    #[serde(default)]
    pub floating_groups: Vec<FloatingGroup>,
    pub home: [Destination; 3],
    pub locale: String,
}
impl Default for LayoutState {
    fn default() -> Self {
        Self {
            version: 3,
            revision: 0,
            next_id: 6,
            root: Some(Node::Split {
                id: 1,
                axis: Axis::Horizontal,
                ratio: 0.22,
                first: Box::new(leaf(2, Panel::Objects)),
                second: Box::new(Node::Split {
                    id: 3,
                    axis: Axis::Horizontal,
                    ratio: 0.70,
                    first: Box::new(leaf(4, Panel::Notebook)),
                    second: Box::new(leaf(5, Panel::Inspector)),
                }),
            }),
            floating: vec![],
            floating_groups: vec![],
            home: [
                Destination {
                    target: Some(Panel::Notebook),
                    zone: Zone::Left,
                },
                Destination {
                    target: Some(Panel::Notebook),
                    zone: Zone::Right,
                },
                Destination {
                    target: Some(Panel::Objects),
                    zone: Zone::Right,
                },
            ],
            locale: "ja".into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Dock {
        panel: Panel,
        target: Option<Panel>,
        zone: Zone,
    },
    Select {
        panel: Panel,
    },
    Return {
        panel: Panel,
    },
    FloatGroup {
        panel: Panel,
    },
    ReturnGroup {
        panel: Panel,
    },
    Resize {
        id: u32,
        ratio: f64,
    },
    Locale {
        locale: String,
    },
    Reset,
}
impl LayoutState {
    pub fn valid(&self) -> bool {
        fn visit(
            n: &Node,
            depth: u32,
            ids: &mut BTreeSet<u32>,
            panels: &mut BTreeSet<Panel>,
        ) -> bool {
            if depth > 5 {
                return false;
            }
            match n {
                Node::Tabs {
                    id,
                    panels: tabs,
                    active,
                } => {
                    *id > 0
                        && ids.insert(*id)
                        && !tabs.is_empty()
                        && tabs.len() <= 3
                        && tabs.contains(active)
                        && tabs.iter().all(|p| panels.insert(*p))
                }
                Node::Split {
                    id,
                    ratio,
                    first,
                    second,
                    ..
                } => {
                    *id > 0
                        && ids.insert(*id)
                        && ratio.is_finite()
                        && (0.1..=0.9).contains(ratio)
                        && visit(first, depth + 1, ids, panels)
                        && visit(second, depth + 1, ids, panels)
                }
            }
        }
        let mut ids = BTreeSet::new();
        let mut panels = BTreeSet::new();
        self.version == 3
            && self.valid_floating_groups()
            && matches!(self.locale.as_str(), "ja" | "en" | "zh-CN")
            && self.floating.len() <= 3
            && self
                .root
                .as_ref()
                .is_none_or(|n| visit(n, 0, &mut ids, &mut panels))
            && self.floating.iter().all(|p| panels.insert(*p))
            && panels.len() == 3
            && ids.len() <= 5
            && self.next_id > ids.last().copied().unwrap_or(0)
            && self
                .home
                .iter()
                .enumerate()
                .all(|(i, d)| d.zone != Zone::Floating && d.target.is_none_or(|p| p.index() != i))
    }
    fn valid_floating_groups(&self) -> bool {
        let mut hosts = BTreeSet::new();
        let mut panels = BTreeSet::new();
        self.floating_groups.len() <= 3
            && self.floating_groups.iter().all(|g| {
                hosts.insert(g.host)
                    && !g.panels.is_empty()
                    && g.panels.len() <= 3
                    && g.panels.contains(&g.active)
                    && g.panels.iter().all(|p| panels.insert(*p))
            })
            && panels == self.floating.iter().copied().collect()
    }
    pub fn floating_group(&self, p: Panel) -> Option<&FloatingGroup> {
        self.floating_groups.iter().find(|g| g.panels.contains(&p))
    }
    pub fn host(&self, p: Panel) -> &'static str {
        self.floating_group(p).map_or("main", |g| g.host.window())
    }
    fn select(&mut self, p: Panel) {
        if let Some(root) = &mut self.root {
            root.select(p);
        }
        for g in &mut self.floating_groups {
            if g.panels.contains(&p) {
                g.active = p;
            }
        }
    }
    pub fn docked(&self, p: Panel) -> bool {
        self.root.as_ref().is_some_and(|n| n.contains(p))
    }
    pub fn tabs(&self, p: Panel) -> Vec<Panel> {
        self.root
            .as_ref()
            .and_then(|n| n.tabs(p))
            .map(Vec::from)
            .or_else(|| self.floating_group(p).map(|g| g.panels.clone()))
            .unwrap_or_else(|| vec![p])
    }
    /// Validate a clone before publication. Invalid requests never consume IDs or revision.
    pub fn apply(&self, command: Command, expected: u64) -> Result<Self, &'static str> {
        if expected != self.revision {
            return Err("layout_conflict");
        }
        let mut next = self.clone();
        match command {
            Command::Dock {
                panel,
                target,
                zone,
            } => next.dock(panel, Destination { target, zone })?,
            Command::Return { panel } => {
                if !next.floating.contains(&panel) {
                    return Ok(next);
                }
                let mut dest = next.home[panel.index()];
                if dest.target.is_none_or(|p| !next.docked(p)) {
                    dest.target = next.root.as_ref().map(Node::first_panel);
                    dest.zone = Zone::Center;
                }
                next.dock(panel, dest)?;
            }
            Command::Select { panel } => next.select(panel),
            Command::FloatGroup { panel } => next.float_group(panel)?,
            Command::ReturnGroup { panel } => next.return_group(panel)?,
            Command::Resize { id, ratio } => {
                if !ratio.is_finite()
                    || !(0.1..=0.9).contains(&ratio)
                    || !next.root.as_mut().is_some_and(|n| n.set_ratio(id, ratio))
                {
                    return Err("invalid_layout");
                }
            }
            Command::Locale { locale } => next.locale = locale,
            Command::Reset => {
                let locale = next.locale;
                next = Self {
                    locale,
                    ..Default::default()
                };
            }
        }
        next.revision = self.revision.checked_add(1).ok_or("invalid_layout")?;
        if !next.valid() {
            return Err("invalid_layout");
        }
        Ok(next)
    }
    fn dock(&mut self, p: Panel, dest: Destination) -> Result<(), &'static str> {
        if dest.target == Some(p) {
            return Err("invalid_layout");
        }
        if dest.zone != Zone::Floating && dest.target.is_some_and(|t| !self.docked(t)) {
            return Err("invalid_layout");
        }
        if dest.zone == Zone::Floating {
            if self.floating_group(p).is_some_and(|g| g.panels.len() == 1) {
                return Ok(());
            }
            if self.docked(p) {
                self.home[p.index()] =
                    self.root
                        .as_ref()
                        .and_then(|n| n.return_slot(p))
                        .unwrap_or(Destination {
                            target: None,
                            zone: Zone::Center,
                        });
            }
        }
        self.root = self.root.take().and_then(|n| n.remove(p));
        self.floating.retain(|v| *v != p);
        for g in &mut self.floating_groups {
            g.panels.retain(|v| *v != p);
            if g.active == p && !g.panels.is_empty() {
                g.active = g.panels[0];
            }
        }
        self.floating_groups.retain(|g| !g.panels.is_empty());
        if dest.zone == Zone::Floating {
            let host = std::iter::once(p)
                .chain(Panel::ALL)
                .find(|h| !self.floating_groups.iter().any(|g| g.host == *h))
                .ok_or("invalid_layout")?;
            self.floating_groups.push(FloatingGroup {
                host,
                panels: vec![p],
                active: p,
            });
            self.floating.push(p);
            return Ok(());
        }
        match (&mut self.root, dest.target) {
            (Some(root), Some(t)) => root.insert(p, t, dest.zone, &mut self.next_id)?,
            (None, _) => self.root = Some(leaf(alloc(&mut self.next_id)?, p)),
            _ => return Err("invalid_layout"),
        }
        Ok(())
    }
    fn float_group(&mut self, p: Panel) -> Result<(), &'static str> {
        if self.floating_group(p).is_some() {
            return Ok(());
        }
        let panels = self.tabs(p);
        // Keep a return slot outside the group, not a soon-to-be-floating sibling tab.
        let mut remaining = self.root.clone();
        for member in panels.iter().filter(|m| **m != p) {
            remaining = remaining.and_then(|n| n.remove(*member));
        }
        let home = remaining
            .as_ref()
            .and_then(|n| n.return_slot(p))
            .unwrap_or(Destination {
                target: None,
                zone: Zone::Center,
            });
        let active = self
            .root
            .as_ref()
            .and_then(|n| active_tab(n, p))
            .ok_or("invalid_layout")?;
        for member in &panels {
            self.root = self.root.take().and_then(|n| n.remove(*member));
            self.home[member.index()] = home;
        }
        let host = std::iter::once(p)
            .chain(Panel::ALL)
            .find(|h| !self.floating_groups.iter().any(|g| g.host == *h))
            .ok_or("invalid_layout")?;
        self.floating.extend(&panels);
        self.floating_groups.push(FloatingGroup {
            host,
            panels,
            active,
        });
        Ok(())
    }
    fn return_group(&mut self, p: Panel) -> Result<(), &'static str> {
        let Some(group) = self.floating_group(p).cloned() else {
            return Ok(());
        };
        let anchor = group.panels[0];
        let mut dest = self.home[anchor.index()];
        if dest.target.is_none_or(|t| !self.docked(t)) {
            dest = Destination {
                target: self.root.as_ref().map(Node::first_panel),
                zone: Zone::Center,
            };
        }
        self.dock(anchor, dest)?;
        let mut previous = anchor;
        for member in group.panels.into_iter().skip(1) {
            self.dock(
                member,
                Destination {
                    target: Some(previous),
                    zone: Zone::Center,
                },
            )?;
            previous = member;
        }
        self.select(group.active);
        Ok(())
    }
    pub fn geometry(&self, size: [u32; 2], scale: f64, inset: u32) -> Geometry {
        let top = ((56. * scale).round() as u32)
            .saturating_add(inset)
            .min(size[1]);
        let mut g = Geometry {
            toolbar: [0, 0, size[0], top],
            body: [0, top, size[0], size[1] - top],
            groups: vec![],
            splits: vec![],
        };
        if let Some(root) = &self.root {
            partition(root, g.body, scale, &mut g);
        }
        g
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let mut s = match value.get("version").and_then(|v| v.as_u64()) {
            Some(3) => serde_json::from_value(value).map_err(|e| e.to_string())?,
            Some(2) => {
                let mut old: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
                old.version = 3;
                old.floating_groups = old
                    .floating
                    .iter()
                    .map(|p| FloatingGroup {
                        host: *p,
                        panels: vec![*p],
                        active: *p,
                    })
                    .collect();
                old
            }
            Some(1) => migrate(value)?,
            _ => return Err("unsupported layout version".into()),
        };
        s.revision = 0;
        if !s.valid() {
            return Err("invalid layout".into());
        }
        Ok(s)
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Group {
    pub id: u32,
    pub rect: [u32; 4],
    pub panels: Vec<Panel>,
    pub active: Panel,
}
#[cfg(test)]
impl Group {
    pub fn content(&self, scale: f64) -> [u32; 4] {
        let [x, y, w, h] = self.rect;
        let header = (96. * scale).round().min(h as f64) as u32;
        [x, y + header, w, h - header]
    }
    pub fn chrome(&self, scale: f64) -> [u32; 4] {
        let [x, y, w, h] = self.rect;
        [x, y, w, (96. * scale).round().min(h as f64) as u32]
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct SplitGeometry {
    pub id: u32,
    pub axis: Axis,
    pub ratio: f64,
    pub parent: [u32; 4],
    pub rect: [u32; 4],
}
#[derive(Clone, Debug, Serialize)]
pub struct Geometry {
    pub toolbar: [u32; 4],
    pub body: [u32; 4],
    pub groups: Vec<Group>,
    pub splits: Vec<SplitGeometry>,
}
fn partition(n: &Node, r: [u32; 4], scale: f64, g: &mut Geometry) {
    match n {
        Node::Tabs { id, panels, active } => g.groups.push(Group {
            id: *id,
            rect: r,
            panels: panels.clone(),
            active: *active,
        }),
        Node::Split {
            id,
            axis,
            ratio,
            first,
            second,
        } => {
            let k = if *axis == Axis::Horizontal { 0 } else { 1 };
            let extent = r[k + 2];
            let gap = (6. * scale).round().min(extent as f64) as u32;
            let room = extent - gap;
            let minimum = ((if k == 0 { 180. } else { 140. }) * scale)
                .round()
                .min((room / 2) as f64) as u32;
            let a = (room as f64 * ratio)
                .round()
                .clamp(minimum as f64, (room - minimum) as f64) as u32;
            let mut first_rect = r;
            first_rect[k + 2] = a;
            let mut splitter = r;
            splitter[k] += a;
            splitter[k + 2] = gap;
            let mut second_rect = r;
            second_rect[k] += a + gap;
            second_rect[k + 2] = room - a;
            g.splits.push(SplitGeometry {
                id: *id,
                axis: *axis,
                ratio: *ratio,
                parent: r,
                rect: splitter,
            });
            partition(first, first_rect, scale, g);
            partition(second, second_rect, scale, g);
        }
    }
}
impl Geometry {
    pub fn drop_target(&self, pointer: [f64; 2], source: Panel) -> Option<Destination> {
        let p = pointer;
        if !p.iter().all(|v| v.is_finite()) {
            return None;
        }
        let inside = |r: [u32; 4]| {
            p[0] >= r[0] as f64
                && p[1] >= r[1] as f64
                && p[0] < (r[0] + r[2]) as f64
                && p[1] < (r[1] + r[3]) as f64
        };
        if !inside(self.body) {
            return Some(Destination {
                target: None,
                zone: Zone::Floating,
            });
        }
        for group in &self.groups {
            if inside(group.rect) {
                let target = if group.active != source {
                    Some(group.active)
                } else {
                    group.panels.iter().copied().find(|v| *v != source)
                };
                let target = target?;
                let [x, y, w, h] = group.rect;
                let u = (p[0] - x as f64) / w as f64;
                let v = (p[1] - y as f64) / h as f64;
                let edges = [
                    (u, Zone::Left),
                    (1. - u, Zone::Right),
                    (v, Zone::Top),
                    (1. - v, Zone::Bottom),
                ];
                let (distance, edge) = edges
                    .into_iter()
                    .min_by(|a, b| a.0.total_cmp(&b.0))
                    .unwrap();
                return Some(Destination {
                    target: Some(target),
                    zone: if distance < 0.23 { edge } else { Zone::Center },
                });
            }
        }
        if self.groups.is_empty() {
            Some(Destination {
                target: None,
                zone: Zone::Center,
            })
        } else {
            None
        }
    }
}
fn migrate(value: serde_json::Value) -> Result<LayoutState, String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Old {
        version: u32,
        #[serde(default)]
        revision: u64,
        panels: [String; 2],
        home: [String; 2],
        widths: [f64; 2],
        locale: String,
    }
    let old: Old = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let _ = (old.version, old.revision);
    if !old
        .panels
        .iter()
        .all(|p| matches!(p.as_str(), "left" | "right" | "floating"))
        || !old
            .home
            .iter()
            .all(|p| matches!(p.as_str(), "left" | "right"))
        || !old
            .widths
            .iter()
            .all(|w| w.is_finite() && (220. ..=600.).contains(w))
    {
        return Err("invalid legacy layout".into());
    }
    let mut s = LayoutState {
        locale: old.locale,
        ..Default::default()
    };
    // Preserve former stacked side groups instead of accidentally turning them into tabs.
    let mut id = 1u32;
    let mut side = |name: &str| -> Option<Node> {
        let panels: Vec<_> = [Panel::Objects, Panel::Inspector]
            .into_iter()
            .filter(|p| old.panels[p.index()] == name)
            .collect();
        let nodes: Vec<_> = panels
            .into_iter()
            .map(|p| {
                let n = leaf(id, p);
                id += 1;
                n
            })
            .collect();
        match nodes.len() {
            0 => None,
            1 => Some(nodes[0].clone()),
            _ => {
                let n = Node::Split {
                    id,
                    axis: Axis::Vertical,
                    ratio: 0.5,
                    first: Box::new(nodes[0].clone()),
                    second: Box::new(nodes[1].clone()),
                };
                id += 1;
                Some(n)
            }
        }
    };
    let left = side("left");
    let right = side("right");
    let mut root = leaf(id, Panel::Notebook);
    id += 1;
    if let Some(right) = right {
        root = Node::Split {
            id,
            axis: Axis::Horizontal,
            ratio: (1. - old.widths[1] / 1000.).clamp(0.1, 0.9),
            first: Box::new(root),
            second: Box::new(right),
        };
        id += 1;
    }
    if let Some(left) = left {
        root = Node::Split {
            id,
            axis: Axis::Horizontal,
            ratio: (old.widths[0] / 1280.).clamp(0.1, 0.9),
            first: Box::new(left),
            second: Box::new(root),
        };
        id += 1;
    }
    s.root = Some(root);
    s.next_id = id;
    for p in [Panel::Objects, Panel::Inspector] {
        if old.panels[p.index()] == "floating" {
            s.floating.push(p);
            s.floating_groups.push(FloatingGroup {
                host: p,
                panels: vec![p],
                active: p,
            });
        }
        s.home[p.index()] = Destination {
            target: Some(Panel::Notebook),
            zone: if old.home[p.index()] == "left" {
                Zone::Left
            } else {
                Zone::Right
            },
        };
    }
    Ok(s)
}
#[cfg(test)]
mod tests;
