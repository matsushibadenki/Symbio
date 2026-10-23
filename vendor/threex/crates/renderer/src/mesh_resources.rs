//! Budgeted custom geometry residency. Defaults are permanently resident outside this budget.
use std::collections::{BTreeMap, BTreeSet};
use threex_core::{GeometryKind, GeometrySpec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeshBudget {
    pub max_bytes: u64,
    pub max_meshes: usize,
}
impl Default for MeshBudget {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_meshes: 128,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshLimit {
    Bytes,
    Count,
    DeviceBuffer,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MeshBudgetStatus {
    /// Vertex/index buffer sizes, including COPY_BUFFER_ALIGNMENT padding.
    pub resident_bytes: u64,
    pub resident_meshes: usize,
    /// These specs use their kind's default mesh until capacity becomes available.
    pub fallback: Vec<(GeometrySpec, MeshLimit)>,
}
#[derive(Clone, Copy, Debug)]
struct Cost {
    vertex: u64,
    index: u64,
}
impl Cost {
    fn of(spec: GeometrySpec) -> Self {
        let w = u64::from(spec.width);
        let h = u64::from(spec.height);
        let vertices = (w + 1) * (h + 1);
        let triangles = match spec.kind {
            GeometryKind::Plane => 2 * w * h,
            GeometryKind::Sphere => 2 * w * (h - 1),
            GeometryKind::Box => unreachable!("Box is permanently resident"),
        };
        let index = (triangles * 3 * if vertices <= 65536 { 2 } else { 4 }).div_ceil(4) * 4;
        Self {
            vertex: vertices * 12,
            index,
        }
    }
    fn total(self) -> u64 {
        self.vertex + self.index
    }
}
pub(crate) struct MeshCache<T> {
    entries: BTreeMap<GeometrySpec, T>,
    status: MeshBudgetStatus,
}
impl<T> Default for MeshCache<T> {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            status: MeshBudgetStatus::default(),
        }
    }
}
impl<T> MeshCache<T> {
    pub fn get(&self, spec: &GeometrySpec) -> Option<&T> {
        self.entries.get(spec)
    }
    pub fn status(&self) -> &MeshBudgetStatus {
        &self.status
    }
    /// Retain existing active meshes first, then admit new specs in canonical order.
    /// All evictions finish before any factory call; rejected meshes are never generated.
    pub fn sync(
        &mut self,
        budget: MeshBudget,
        max_buffer: u64,
        active: impl IntoIterator<Item = GeometrySpec>,
        mut create: impl FnMut(GeometrySpec) -> T,
    ) {
        let active: BTreeSet<_> = active
            .into_iter()
            .filter(|s| s.flags() & !15 != 0)
            .collect();
        let ordered: Vec<_> = self
            .entries
            .keys()
            .copied()
            .filter(|s| active.contains(s))
            .chain(
                active
                    .iter()
                    .copied()
                    .filter(|s| !self.entries.contains_key(s)),
            )
            .collect();
        let mut admitted = BTreeSet::new();
        let mut status = MeshBudgetStatus::default();
        for spec in ordered {
            let cost = Cost::of(spec);
            let limit = if cost.vertex > max_buffer || cost.index > max_buffer {
                Some(MeshLimit::DeviceBuffer)
            } else if status.resident_meshes >= budget.max_meshes {
                Some(MeshLimit::Count)
            } else if cost.total() > budget.max_bytes.saturating_sub(status.resident_bytes) {
                Some(MeshLimit::Bytes)
            } else {
                None
            };
            if let Some(reason) = limit {
                status.fallback.push((spec, reason));
            } else {
                status.resident_meshes += 1;
                status.resident_bytes += cost.total();
                admitted.insert(spec);
            }
        }
        self.entries.retain(|spec, _| admitted.contains(spec));
        for spec in admitted {
            self.entries.entry(spec).or_insert_with(|| create(spec));
        }
        self.status = status;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plane(w: u16, h: u16) -> GeometrySpec {
        GeometrySpec::new(GeometryKind::Plane, [w, h]).unwrap()
    }
    #[test]
    fn estimates_match_uploaded_vertex_index_sizes_and_index_width_boundary() {
        for kind in [GeometryKind::Plane, GeometryKind::Sphere] {
            for segments in [[3, 2], [7, 9], [32, 16], [255, 255], [256, 256]] {
                let spec = GeometrySpec::new(kind, segments).unwrap();
                let mesh = crate::geometry::geometry(spec);
                let cost = Cost::of(spec);
                assert_eq!(cost.vertex, (mesh.vertices.len() * 12) as u64);
                let bytes = mesh.indices.len() * if mesh.vertices.len() <= 65536 { 2 } else { 4 };
                assert_eq!(cost.index, bytes.div_ceil(4) as u64 * 4);
            }
        }
    }
    #[test]
    fn exact_limits_deduplicate_defaults_and_skip_oversized_without_creation() {
        let a = plane(2, 3);
        let b = plane(256, 256);
        let default = plane(1, 1);
        let bytes = Cost::of(a).total();
        let mut cache = MeshCache::default();
        let budget = MeshBudget {
            max_bytes: bytes,
            max_meshes: 1,
        };
        cache.sync(budget, u64::MAX, [a, a, default, b], |s| {
            assert_eq!(s, a);
            s
        });
        assert_eq!(cache.status.resident_bytes, bytes);
        assert_eq!(cache.status.resident_meshes, 1);
        assert_eq!(cache.status.fallback, vec![(b, MeshLimit::Count)]);
        cache.sync(
            MeshBudget {
                max_bytes: bytes - 1,
                max_meshes: 5,
            },
            u64::MAX,
            [a],
            |_| panic!("rejected creation"),
        );
        assert_eq!(cache.status.fallback, vec![(a, MeshLimit::Bytes)]);
        let c = Cost::of(a);
        cache.sync(budget, c.vertex.max(c.index) - 1, [a], |_| {
            panic!("device limit")
        });
        assert_eq!(cache.status.fallback, vec![(a, MeshLimit::DeviceBuffer)]);
        cache.sync(budget, c.vertex.max(c.index), [a], |s| s);
        assert_eq!(cache.status.resident_meshes, 1);
        // A large rejected mesh does not block a later smaller one.
        let sphere = GeometrySpec::new(GeometryKind::Sphere, [3, 2]).unwrap();
        cache.sync(
            MeshBudget {
                max_bytes: Cost::of(sphere).total(),
                max_meshes: 2,
            },
            u64::MAX,
            [b, sphere],
            |s| s,
        );
        assert!(cache.get(&sphere).is_some());
        assert_eq!(cache.status.fallback, vec![(b, MeshLimit::Bytes)]);
    }
    #[test]
    fn release_precedes_create_capacity_reuses_and_active_mesh_keeps_identity() {
        use std::{cell::Cell, rc::Rc};
        struct Resource {
            live: Rc<Cell<usize>>,
            id: usize,
        }
        impl Drop for Resource {
            fn drop(&mut self) {
                self.live.set(self.live.get() - 1);
            }
        }
        let live = Rc::new(Cell::new(0));
        let serial = Cell::new(0);
        let create = |_| {
            assert_eq!(live.get(), 0, "old mesh must be dropped before replacement");
            live.set(live.get() + 1);
            serial.set(serial.get() + 1);
            Resource {
                live: live.clone(),
                id: serial.get(),
            }
        };
        let (a, b) = (plane(2, 3), plane(5, 7));
        let budget = MeshBudget {
            max_bytes: 10000,
            max_meshes: 1,
        };
        let mut cache = MeshCache::default();
        cache.sync(budget, u64::MAX, [a], create);
        let id = cache.get(&a).unwrap().id;
        for _ in 0..10 {
            cache.sync(budget, u64::MAX, [b, a, a], |_| {
                panic!("active resident should be reused")
            });
            assert_eq!(cache.get(&a).unwrap().id, id);
            assert_eq!(cache.status.fallback, vec![(b, MeshLimit::Count)]);
        }
        cache.sync(budget, u64::MAX, [b], create);
        assert_ne!(cache.get(&b).unwrap().id, id);
        assert_eq!(live.get(), 1);
        cache.sync(
            MeshBudget {
                max_bytes: 0,
                max_meshes: 0,
            },
            u64::MAX,
            [b],
            |_| panic!(),
        );
        assert_eq!(live.get(), 0);
        cache.sync(budget, u64::MAX, [b], create);
        cache.sync(budget, u64::MAX, [], |_| panic!());
        assert_eq!(cache.status.resident_bytes, 0);
        assert!(cache.status.fallback.is_empty());
        assert_eq!(live.get(), 0);
        cache.sync(budget, u64::MAX, [a], create);
        drop(cache);
        assert_eq!(live.get(), 0);
    }
    #[test]
    fn budget_changes_leave_sparse_uploads_and_gather_cache_untouched() {
        use crate::{
            batching::GeometryBatches,
            gather::{PlanCache, Prepared},
        };
        use threex_core::Instance;
        let specs = [plane(2, 3), plane(5, 7)];
        let mut data: Vec<_> = (0..1200)
            .map(|i| Instance {
                model: threex_core::identity(),
                color: [i as f32, 0., 0., specs[i % 2].tag() as f32],
            })
            .collect();
        let mut batches = GeometryBatches::default();
        batches.sync(&data, []);
        let revision = batches.revision(1);
        let limits = wgpu::Limits {
            max_storage_buffer_binding_size: 2560,
            ..Default::default()
        };
        let mut plans = PlanCache::default();
        assert!(matches!(
            plans.prepare(&limits, [100000, 100000], &batches, 1, true),
            Prepared::Gather { upload: true, .. }
        ));
        let mut cache = MeshCache::default();
        for cap in [0, 1, 2, 1, 0, 2] {
            cache.sync(
                MeshBudget {
                    max_bytes: 10000,
                    max_meshes: cap,
                },
                u64::MAX,
                specs,
                |s| s,
            );
            assert_eq!(cache.status.resident_meshes, cap);
            assert_eq!(batches.revision(1), revision);
            assert!(matches!(
                plans.prepare(&limits, [100000, 100000], &batches, 1, true),
                Prepared::Gather { upload: false, .. }
            ));
        }
        data[0].color[0] += 1.;
        batches.sync(&data, std::iter::once(0..1));
        assert_eq!(batches.transfer_count(1, false), 1);
        assert_eq!(batches.revision(1), revision);
        assert!(matches!(
            plans.prepare(&limits, [100000, 100000], &batches, 1, false),
            Prepared::Copies(_)
        ));
        assert!(matches!(
            plans.prepare(&limits, [100000, 100000], &batches, 1, true),
            Prepared::Gather { upload: false, .. }
        ));
        data[0].color[3] = plane(9, 11).tag() as f32;
        batches.sync(&data, std::iter::once(0..1));
        cache.sync(
            MeshBudget::default(),
            u64::MAX,
            batches.geometry_draws().map(|(s, _)| s),
            |s| s,
        );
        assert_eq!(cache.status.resident_meshes, 3);
        assert_ne!(batches.revision(1), revision);
        assert!(matches!(
            plans.prepare(&limits, [100000, 100000], &batches, 1, true),
            Prepared::Gather { upload: true, .. }
        ));
    }
}
