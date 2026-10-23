use super::*;
fn apply(s: &mut LayoutState, c: Command) {
    *s = s.apply(c, s.revision).unwrap();
    assert!(s.valid());
}
fn dock(s: &mut LayoutState, panel: Panel, target: Option<Panel>, zone: Zone) {
    apply(
        s,
        Command::Dock {
            panel,
            target,
            zone,
        },
    );
}
#[test]
fn tabs_prune_select_and_split_in_all_directions() {
    for zone in [Zone::Left, Zone::Right, Zone::Top, Zone::Bottom] {
        let mut s = LayoutState::default();
        dock(&mut s, Panel::Inspector, Some(Panel::Objects), Zone::Center);
        assert_eq!(
            s.tabs(Panel::Objects),
            vec![Panel::Objects, Panel::Inspector]
        );
        apply(
            &mut s,
            Command::Select {
                panel: Panel::Objects,
            },
        );
        let g = s.geometry([1200, 800], 1., 32);
        assert!(
            g.groups
                .iter()
                .any(|g| g.active == Panel::Objects && g.panels.len() == 2)
        );
        dock(&mut s, Panel::Inspector, Some(Panel::Objects), zone);
        let g = s.geometry([1200, 800], 1., 32);
        assert_eq!(g.groups.len(), 3);
        assert_eq!(g.splits.len(), 2);
        let a = g
            .groups
            .iter()
            .find(|g| g.active == Panel::Inspector)
            .unwrap()
            .rect;
        let b = g
            .groups
            .iter()
            .find(|g| g.active == Panel::Objects)
            .unwrap()
            .rect;
        match zone {
            Zone::Left => assert!(a[0] < b[0]),
            Zone::Right => assert!(a[0] > b[0]),
            Zone::Top => assert!(a[1] < b[1]),
            Zone::Bottom => assert!(a[1] > b[1]),
            _ => unreachable!(),
        }
    }
}
#[test]
fn floating_all_panels_and_return_fallback_preserve_identity() {
    let mut s = LayoutState::default();
    for p in Panel::ALL {
        dock(&mut s, p, None, Zone::Floating);
    }
    assert!(s.root.is_none());
    assert_eq!(s.floating.len(), 3);
    for p in [Panel::Notebook, Panel::Inspector, Panel::Objects] {
        apply(&mut s, Command::Return { panel: p });
    }
    assert!(s.floating.is_empty());
    assert!(Panel::ALL.into_iter().all(|p| s.docked(p)));
    apply(&mut s, Command::Reset);
    assert_eq!(s.root, LayoutState::default().root);
}
#[test]
fn tab_float_returns_to_group_and_reorder_is_deterministic() {
    let mut s = LayoutState::default();
    dock(&mut s, Panel::Inspector, Some(Panel::Objects), Zone::Center);
    dock(
        &mut s,
        Panel::Notebook,
        Some(Panel::Inspector),
        Zone::Center,
    );
    dock(&mut s, Panel::Objects, Some(Panel::Notebook), Zone::Center);
    assert_eq!(
        s.tabs(Panel::Objects),
        vec![Panel::Inspector, Panel::Notebook, Panel::Objects]
    );
    dock(&mut s, Panel::Notebook, None, Zone::Floating);
    apply(
        &mut s,
        Command::Return {
            panel: Panel::Notebook,
        },
    );
    assert_eq!(s.tabs(Panel::Objects).len(), 3);
}
#[test]
fn invalid_and_stale_commands_are_atomic() {
    let s = LayoutState::default();
    let original = serde_json::to_vec(&s).unwrap();
    for command in [
        Command::Resize {
            id: 1,
            ratio: f64::NAN,
        },
        Command::Resize {
            id: 999,
            ratio: 0.5,
        },
        Command::Resize { id: 1, ratio: 0.99 },
        Command::Dock {
            panel: Panel::Objects,
            target: Some(Panel::Objects),
            zone: Zone::Left,
        },
        Command::Locale {
            locale: "bad".into(),
        },
    ] {
        assert!(s.apply(command, 0).is_err());
    }
    assert!(s.apply(Command::Reset, 1).is_err());
    assert_eq!(serde_json::to_vec(&s).unwrap(), original);
}
#[test]
fn partitions_cover_body_without_overlap_at_all_scales() {
    for scale in [1., 1.25, 2.] {
        for size in [[0, 0], [1, 1], [400, 240], [800, 480], [2560, 1600]] {
            for zone in [Zone::Center, Zone::Top, Zone::Left] {
                let mut s = LayoutState::default();
                dock(&mut s, Panel::Inspector, Some(Panel::Objects), zone);
                let g = s.geometry(size, scale, 32);
                let rectangles: Vec<_> = g
                    .groups
                    .iter()
                    .map(|g| g.rect)
                    .chain(g.splits.iter().map(|s| s.rect))
                    .collect();
                assert_eq!(
                    rectangles
                        .iter()
                        .map(|r| r[2] as u64 * r[3] as u64)
                        .sum::<u64>(),
                    g.body[2] as u64 * g.body[3] as u64
                );
                for (i, a) in rectangles.iter().enumerate() {
                    assert!(a[0] + a[2] <= size[0] && a[1] + a[3] <= size[1]);
                    for b in &rectangles[i + 1..] {
                        if a[2] * a[3] > 0 && b[2] * b[3] > 0 {
                            assert!(
                                a[0] + a[2] <= b[0]
                                    || b[0] + b[2] <= a[0]
                                    || a[1] + a[3] <= b[1]
                                    || b[1] + b[3] <= a[1]
                            );
                        }
                    }
                }
                for group in &g.groups {
                    assert_eq!(
                        group.chrome(scale)[3] + group.content(scale)[3],
                        group.rect[3]
                    );
                }
            }
        }
    }
}
#[test]
fn drop_targets_choose_center_edges_and_empty_workspace() {
    let s = LayoutState::default();
    let g = s.geometry([1200, 800], 1., 32);
    let r = g
        .groups
        .iter()
        .find(|g| g.active == Panel::Notebook)
        .unwrap()
        .rect;
    for (u, v, zone) in [
        (0.5, 0.5, Zone::Center),
        (0.01, 0.5, Zone::Left),
        (0.99, 0.5, Zone::Right),
        (0.5, 0.01, Zone::Top),
        (0.5, 0.99, Zone::Bottom),
    ] {
        assert_eq!(
            g.drop_target(
                [r[0] as f64 + r[2] as f64 * u, r[1] as f64 + r[3] as f64 * v],
                Panel::Objects
            ),
            Some(Destination {
                target: Some(Panel::Notebook),
                zone
            })
        );
    }
    assert_eq!(
        g.drop_target([-10., 300.], Panel::Objects).unwrap().zone,
        Zone::Floating
    );
    assert!(g.drop_target([f64::NAN, 300.], Panel::Objects).is_none());
    let mut s = s;
    for p in Panel::ALL {
        dock(&mut s, p, None, Zone::Floating);
    }
    assert_eq!(
        s.geometry([800, 600], 1., 32)
            .drop_target([300., 300.], Panel::Notebook),
        Some(Destination {
            target: None,
            zone: Zone::Center
        })
    );
}
#[test]
fn persistence_migrates_legacy_and_rejects_corrupt_trees() {
    let old=br#"{"version":1,"revision":8,"panels":["floating","left"],"home":["right","left"],"widths":[270,300],"locale":"en"}"#;
    let s = LayoutState::decode(old).unwrap();
    assert_eq!(s.revision, 0);
    assert_eq!(s.floating, vec![Panel::Objects]);
    assert_eq!(s.home[0].zone, Zone::Right);
    assert_eq!(
        LayoutState::decode(&serde_json::to_vec(&s).unwrap()).unwrap(),
        s
    );
    let mut invalid = s.clone();
    invalid.floating.push(Panel::Inspector);
    assert!(!invalid.valid());
    assert!(LayoutState::decode(&serde_json::to_vec(&invalid).unwrap()).is_err());
    let mut invalid = s;
    invalid.next_id = 1;
    assert!(!invalid.valid());
    assert!(LayoutState::decode(br#"{"version":42}"#).is_err());
}
#[test]
fn long_transition_sequence_keeps_each_panel_exactly_once() {
    let mut s = LayoutState::default();
    let mut seed = 71u32;
    for _ in 0..1000 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let p = Panel::ALL[(seed % 3) as usize];
        let candidates: Vec<_> = Panel::ALL
            .into_iter()
            .filter(|t| *t != p && s.docked(*t))
            .collect();
        if seed.is_multiple_of(5) {
            dock(&mut s, p, None, Zone::Floating);
        } else if candidates.is_empty() {
            apply(&mut s, Command::Return { panel: p });
        } else {
            let target = candidates[(seed as usize / 3) % candidates.len()];
            let zone = [
                Zone::Center,
                Zone::Left,
                Zone::Right,
                Zone::Top,
                Zone::Bottom,
            ][(seed as usize / 7) % 5];
            dock(&mut s, p, Some(target), zone);
        }
    }
}

#[test]
fn floating_group_keeps_order_selection_and_returns_to_original_side() {
    let mut s = LayoutState::default();
    s = s
        .apply(
            Command::Dock {
                panel: Panel::Inspector,
                target: Some(Panel::Objects),
                zone: Zone::Center,
            },
            s.revision,
        )
        .unwrap();
    let original = s.root.clone();
    s = s
        .apply(
            Command::FloatGroup {
                panel: Panel::Objects,
            },
            s.revision,
        )
        .unwrap();
    assert_eq!(
        s.tabs(Panel::Objects),
        vec![Panel::Objects, Panel::Inspector]
    );
    assert_eq!(s.floating_groups[0].active, Panel::Inspector);
    assert_eq!(s.host(Panel::Inspector), "float-objects");
    assert!(s.docked(Panel::Notebook));
    s = s
        .apply(
            Command::Select {
                panel: Panel::Objects,
            },
            s.revision,
        )
        .unwrap();
    assert_eq!(s.floating_groups[0].active, Panel::Objects);
    let restored = LayoutState::decode(&serde_json::to_vec(&s).unwrap()).unwrap();
    assert_eq!(restored.floating_groups, s.floating_groups);
    s = s
        .apply(
            Command::ReturnGroup {
                panel: Panel::Inspector,
            },
            s.revision,
        )
        .unwrap();
    assert!(s.floating.is_empty());
    assert_eq!(
        s.tabs(Panel::Objects),
        vec![Panel::Objects, Panel::Inspector]
    );
    let g = s.geometry([1000, 800], 1., 0).groups;
    assert!(g[0].panels.contains(&Panel::Objects));
    assert_eq!(g[0].active, Panel::Objects);
    assert!(original.is_some());
}

#[test]
fn whole_workspace_float_can_detach_host_member_and_return_without_collision() {
    let mut s = LayoutState::default();
    for panel in [Panel::Inspector, Panel::Notebook] {
        s = s
            .apply(
                Command::Dock {
                    panel,
                    target: Some(Panel::Objects),
                    zone: Zone::Center,
                },
                s.revision,
            )
            .unwrap();
    }
    s = s
        .apply(
            Command::FloatGroup {
                panel: Panel::Objects,
            },
            s.revision,
        )
        .unwrap();
    assert!(s.root.is_none());
    let order = s.tabs(Panel::Objects);
    s = s
        .apply(
            Command::Dock {
                panel: Panel::Objects,
                target: None,
                zone: Zone::Floating,
            },
            s.revision,
        )
        .unwrap();
    assert_ne!(s.host(Panel::Objects), s.host(Panel::Notebook));
    assert_eq!(s.tabs(Panel::Objects), vec![Panel::Objects]);
    assert!(s.valid());
    s = s
        .apply(
            Command::ReturnGroup {
                panel: Panel::Notebook,
            },
            s.revision,
        )
        .unwrap();
    s = s
        .apply(
            Command::Return {
                panel: Panel::Objects,
            },
            s.revision,
        )
        .unwrap();
    assert!(s.floating_groups.is_empty());
    assert_eq!(s.tabs(Panel::Objects).len(), order.len());
    assert!(s.valid());
}

#[test]
fn v2_single_floats_migrate_and_v3_rejects_ambiguous_groups() {
    let s = LayoutState::default()
        .apply(
            Command::Dock {
                panel: Panel::Notebook,
                target: None,
                zone: Zone::Floating,
            },
            0,
        )
        .unwrap();
    let mut old = serde_json::to_value(&s).unwrap();
    old["version"] = 2.into();
    old.as_object_mut().unwrap().remove("floating_groups");
    let migrated = LayoutState::decode(&serde_json::to_vec(&old).unwrap()).unwrap();
    assert_eq!(migrated.version, 3);
    assert_eq!(migrated.host(Panel::Notebook), "float-notebook");
    for broken in [
        serde_json::json!([]),
        serde_json::json!([{ "host":"objects", "panels":["notebook","notebook"], "active":"notebook" }]),
        serde_json::json!([{ "host":"objects", "panels":["notebook"], "active":"objects" }]),
    ] {
        let mut value = serde_json::to_value(&s).unwrap();
        value["floating_groups"] = broken;
        assert!(LayoutState::decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}
