use knave_ui::{TextSpan, TextStyle, toolkit::*};
struct Metrics;
impl TextMeasurer for Metrics {
    fn measure(&mut self, text: &str, style: &TextStyle, width: f32) -> TextLayout {
        TextLayout {
            size: [
                (text.chars().count() as f32 * 8.0).min(width),
                style.line_height,
            ],
            carets: text
                .char_indices()
                .enumerate()
                .map(|(n, (byte, _))| Caret {
                    byte,
                    x: n as f32 * 8.0,
                    y: 0.0,
                    height: 18.0,
                })
                .chain([Caret {
                    byte: text.len(),
                    x: text.chars().count() as f32 * 8.0,
                    y: 0.0,
                    height: 18.0,
                }])
                .collect(),
        }
    }
    fn measure_spans(&mut self, spans: &[TextSpan], style: &TextStyle, width: f32) -> TextLayout {
        self.measure(
            &spans.iter().map(|s| s.text.as_str()).collect::<String>(),
            style,
            width,
        )
    }
}
fn leaf(id: u64, w: Widget) -> Element {
    Element::new(id, w).layout(Layout {
        height: Length::Px(40.0),
        ..Layout::default()
    })
}
fn scene(children: Vec<Element>) -> Scene {
    let mut scene = Scene::new(Element::new(0, Widget::Panel).children(children)).unwrap();
    scene.layout([400.0, 400.0], &mut Metrics);
    scene
}
fn click(s: &mut Scene, p: [f32; 2]) -> Option<Action> {
    s.event(Input::PointerDown(p));
    s.event(Input::PointerUp(p)).action
}
fn key(s: &mut Scene, key: Key, shift: bool) -> Option<Action> {
    s.event(Input::Key {
        key,
        pressed: true,
        repeat: false,
        modifiers: KeyModifiers {
            shift,
            ..Default::default()
        },
    })
    .action
}
#[test]
fn press_leave_reenter_and_release_is_one_activation() {
    let mut s = scene(vec![leaf(1, Widget::Button("Button".into()))]);
    s.event(Input::PointerDown([20.0, 20.0]));
    s.event(Input::PointerMove([450.0, 20.0]));
    assert_eq!(s.capture(), Some(ElementId(1)));
    assert!(!s.interaction(ElementId(1)).unwrap().pressed);
    s.event(Input::PointerMove([20.0, 20.0]));
    assert_eq!(
        s.event(Input::PointerUp([20.0, 20.0])).action,
        Some(Action::Activated(ElementId(1)))
    );
    assert_eq!(s.event(Input::PointerUp([20.0, 20.0])).action, None);
    s.event(Input::PointerDown([20.0, 20.0]));
    assert_eq!(s.event(Input::PointerUp([450.0, 20.0])).action, None);
}
#[test]
fn slider_captures_outside_and_cancel_restores_value() {
    let mut s = scene(vec![leaf(
        1,
        Widget::Slider(Slider::new(0.0, 100.0, 50.0, Some(1.0)).unwrap()),
    )]);
    s.event(Input::PointerDown([200.0, 20.0]));
    assert!(matches!(
        s.event(Input::PointerMove([900.0, 20.0])).action,
        Some(Action::ValueChanged(_, 100.0))
    ));
    assert_eq!(
        key(&mut s, Key::Escape, false),
        Some(Action::Cancelled(ElementId(1)))
    );
    let Some(Widget::Slider(slider)) = s.widget(ElementId(1)) else {
        panic!()
    };
    assert_eq!(slider.value(), 50.0);
    s.event(Input::PointerDown([200.0, 20.0]));
    s.set_enabled(ElementId(1), false).unwrap();
    assert_eq!(s.capture(), None);
}
#[test]
fn dropdown_skips_separators_disabled_items_and_preserves_option_identity() {
    let mut s = scene(vec![leaf(
        1,
        Widget::Dropdown {
            label: "Select".into(),
            selected: None,
            items: vec![
                MenuItem::option(10, "First"),
                MenuItem::Separator,
                MenuItem::Option {
                    id: 20,
                    label: "Disabled".into(),
                    enabled: false,
                },
                MenuItem::option(30, "Last"),
            ],
        },
    )]);
    click(&mut s, [20.0, 20.0]);
    key(&mut s, Key::Down, false);
    assert_eq!(
        key(&mut s, Key::Enter, false),
        Some(Action::Selected(ElementId(1), 30))
    );
    assert!(matches!(
        s.widget(ElementId(1)),
        Some(Widget::Dropdown {
            selected: Some(30),
            ..
        })
    ));
}
#[test]
fn modal_traps_focus_blocks_background_and_restores_opener() {
    let mut popup = Element::new(10, Widget::Popup { modal: true })
        .layout(Layout {
            width: Length::Px(180.0),
            height: Length::Px(100.0),
            ..Layout::default()
        })
        .children(vec![
            leaf(11, Widget::Button("OK".into())),
            leaf(12, Widget::Button("Cancel".into())),
        ]);
    popup.visible = false;
    let mut s = scene(vec![leaf(1, Widget::Button("Open".into())), popup]);
    click(&mut s, [20.0, 20.0]);
    s.open_popup(ElementId(10)).unwrap();
    s.layout([400.0, 400.0], &mut Metrics);
    assert_eq!(s.hit_test([20.0, 20.0]), None);
    assert_eq!(s.focus(), Some(ElementId(11)));
    key(&mut s, Key::Tab, true);
    assert_eq!(s.focus(), Some(ElementId(12)));
    key(&mut s, Key::Tab, false);
    assert_eq!(s.focus(), Some(ElementId(11)));
    key(&mut s, Key::Escape, false);
    assert_eq!(s.focus(), Some(ElementId(1)));
}
#[test]
fn clipping_prevents_invisible_rows_from_receiving_input_and_scroll_reveals_them() {
    let scroller = Element::new(1, Widget::Scroll)
        .layout(Layout {
            height: Length::Px(60.0),
            ..Layout::default()
        })
        .children(vec![
            leaf(2, Widget::Button("A".into())),
            leaf(3, Widget::Button("B".into())),
        ]);
    let mut s = scene(vec![scroller]);
    assert_eq!(s.hit_test([20.0, 75.0]), None);
    s.event(Input::Scroll {
        position: [20.0, 20.0],
        delta: [0.0, 40.0],
    });
    s.layout([400.0, 400.0], &mut Metrics);
    assert_eq!(s.hit_test([20.0, 40.0]), Some(ElementId(3)));
}
#[test]
fn hover_and_slider_motion_do_not_invalidate_layout() {
    let mut s = scene(vec![
        leaf(1, Widget::Button("A".into())),
        leaf(2, Widget::Slider(Slider::new(0.0, 1.0, 0.5, None).unwrap())),
    ]);
    s.display_list([400.0, 400.0], &mut Metrics);
    let before = s.stats();
    s.event(Input::PointerMove([20.0, 20.0]));
    s.display_list([400.0, 400.0], &mut Metrics);
    assert_eq!(s.stats().layouts, before.layouts);
    s.event(Input::PointerDown([200.0, 68.0]));
    s.event(Input::PointerMove([300.0, 68.0]));
    s.display_list([400.0, 400.0], &mut Metrics);
    assert_eq!(s.stats().layouts, before.layouts);
    let before = s.stats();
    s.display_list([400.0, 400.0], &mut Metrics);
    assert_eq!(s.stats(), before);
}
#[test]
fn text_editor_handles_graphemes_selection_and_limits() {
    let mut edit = TextEdit::new("a👨‍👩‍👧‍👦e\u{301}").unwrap();
    edit.delete(false);
    assert_eq!(edit.text(), "a👨‍👩‍👧‍👦");
    edit.delete(false);
    assert_eq!(edit.text(), "a");
    edit.select_all();
    edit.insert("नेपाली");
    assert_eq!(edit.text(), "नेपाली");
    assert!(!edit.insert(&"x".repeat(MAX_TEXT_BYTES + 1)));
}
#[test]
fn removed_capture_does_not_activate_a_reused_or_other_id() {
    let mut s = scene(vec![
        leaf(1, Widget::Button("A".into())),
        leaf(2, Widget::Button("B".into())),
    ]);
    s.event(Input::PointerDown([20.0, 20.0]));
    s.remove(ElementId(1)).unwrap();
    s.layout([400.0, 400.0], &mut Metrics);
    assert_eq!(s.capture(), None);
    assert_eq!(s.event(Input::PointerUp([20.0, 20.0])).action, None);
}
#[test]
fn invalid_scene_and_slider_inputs_are_rejected() {
    assert!(matches!(
        Scene::new(Element::new(1, Widget::Panel).children(vec![leaf(1, Widget::Panel)])),
        Err(UiError::DuplicateId(_))
    ));
    assert!(Slider::new(0.0, 1.0, f64::NAN, None).is_err());
    assert!(Slider::new(1.0, 1.0, 1.0, None).is_err());
    assert!(Slider::new(0.0, 1.0, 0.0, Some(0.0)).is_err());
}

#[test]
fn opaque_or_disabled_top_elements_do_not_click_through_and_card_children_target_card() {
    let mut cover = leaf(2, Widget::Panel);
    cover.style.background = Some(knave_ui::ShapePaint::default());
    let root = Element::new(0, Widget::Panel)
        .layout(Layout {
            flow: Flow::Overlay,
            ..Default::default()
        })
        .children(vec![leaf(1, Widget::Button("Under".into())), cover]);
    let mut s = Scene::new(root).unwrap();
    s.layout([400.0, 400.0], &mut Metrics);
    assert_eq!(s.hit_test([20.0, 20.0]), None);
    s.set_widget(ElementId(2), Widget::Button("Disabled cover".into()))
        .unwrap();
    s.set_enabled(ElementId(2), false).unwrap();
    s.layout([400.0, 400.0], &mut Metrics);
    assert_eq!(s.hit_test([20.0, 20.0]), None);
    let mut s = scene(vec![
        leaf(
            1,
            Widget::Selectable {
                label: String::new(),
                selected: false,
            },
        )
        .children(vec![leaf(2, Widget::Text("Card label".into()))]),
    ]);
    assert_eq!(
        click(&mut s, [20.0, 20.0]),
        Some(Action::Activated(ElementId(1)))
    );
}

#[test]
fn drag_has_threshold_and_cancel_restores_origin() {
    let mut s = scene(vec![leaf(1, Widget::Draggable("Drag".into()))]);
    s.event(Input::PointerDown([20.0, 20.0]));
    assert_eq!(s.event(Input::PointerMove([21.0, 21.0])).action, None);
    assert_eq!(
        s.event(Input::PointerMove([30.0, 30.0])).action,
        Some(Action::DragStarted(ElementId(1)))
    );
    s.layout([400.0, 400.0], &mut Metrics);
    assert_eq!(s.bounds(ElementId(1)).unwrap().x, 10.0);
    assert_eq!(
        s.event(Input::FocusLost).action,
        Some(Action::Cancelled(ElementId(1)))
    );
    s.layout([400.0, 400.0], &mut Metrics);
    assert_eq!(s.bounds(ElementId(1)).unwrap().x, 0.0);
}

#[test]
fn losing_focus_rejects_late_text_and_paste_and_restores_visual_focus() {
    let mut s = scene(vec![leaf(
        1,
        Widget::TextInput(TextEdit::new("a").unwrap()),
    )]);
    click(&mut s, [20.0, 20.0]);
    s.event(Input::FocusLost);
    assert!(s.text_input_state().is_none());
    assert!(s.event(Input::Text("late".into())).action.is_none());
    assert!(
        s.event(Input::PasteTo {
            target: ElementId(1),
            text: "late".into()
        })
        .action
        .is_none()
    );
    s.event(Input::FocusGained);
    assert!(s.interaction(ElementId(1)).unwrap().focused);
}
#[test]
fn scene_mutation_keeps_unrelated_modal_state_and_hiding_modal_restores_opener() {
    let mut popup = Element::new(10, Widget::Popup { modal: true })
        .children(vec![leaf(11, Widget::Button("OK".into()))]);
    popup.visible = false;
    let mut s = scene(vec![
        leaf(1, Widget::Button("Open".into())),
        leaf(2, Widget::Button("Remove".into())),
        popup,
    ]);
    click(&mut s, [20.0, 20.0]);
    s.open_popup(ElementId(10)).unwrap();
    s.remove(ElementId(2)).unwrap();
    s.layout([400.0, 400.0], &mut Metrics);
    assert_eq!(s.focus(), Some(ElementId(11)));
    assert_eq!(s.hit_test([20.0, 20.0]), None);
    s.set_visible(ElementId(10), false).unwrap();
    assert_eq!(s.focus(), Some(ElementId(1)));
    s.layout([400.0, 400.0], &mut Metrics);
    assert_eq!(s.hit_test([20.0, 20.0]), Some(ElementId(1)));
}
#[test]
fn row_fill_text_is_measured_at_its_allocated_width() {
    struct WrapMetrics;
    impl TextMeasurer for WrapMetrics {
        fn measure(&mut self, text: &str, _: &TextStyle, width: f32) -> TextLayout {
            TextLayout {
                size: [width, (text.len() as f32 * 10.0 / width).ceil() * 20.0],
                carets: vec![],
            }
        }
        fn measure_spans(&mut self, _: &[TextSpan], _: &TextStyle, _: f32) -> TextLayout {
            unreachable!()
        }
    }
    let root = Element::new(0, Widget::Panel)
        .layout(Layout {
            flow: Flow::Row,
            gap: 0.0,
            ..Default::default()
        })
        .children(vec![
            Element::new(1, Widget::Text("x".repeat(100))),
            Element::new(2, Widget::Text("x".repeat(100))),
        ]);
    let mut s = Scene::new(root).unwrap();
    s.layout([200.0, 1000.0], &mut WrapMetrics);
    assert_eq!(s.bounds(ElementId(1)).unwrap().width, 100.0);
    assert_eq!(s.bounds(ElementId(1)).unwrap().height, 200.0);
}

#[test]
fn scrolling_reuses_measurements_and_slider_endpoints_keep_focus() {
    let mut s = scene(vec![
        Element::new(1, Widget::Scroll)
            .layout(Layout {
                height: Length::Px(60.0),
                ..Default::default()
            })
            .children(vec![
                leaf(2, Widget::Text("row".into())),
                leaf(3, Widget::Text("row".into())),
                leaf(4, Widget::Text("row".into())),
            ]),
        leaf(5, Widget::Slider(Slider::new(0.0, 1.0, 0.0, None).unwrap())),
        leaf(6, Widget::Button("next".into())),
    ]);
    let measured = s.stats().measured_texts;
    s.event(Input::Scroll {
        position: [20.0, 20.0],
        delta: [0.0, 30.0],
    });
    s.layout([400.0, 400.0], &mut Metrics);
    assert_eq!(s.stats().measured_texts, measured);
    key(&mut s, Key::Tab, false);
    assert_eq!(s.focus(), Some(ElementId(5)));
    key(&mut s, Key::Down, false);
    assert_eq!(s.focus(), Some(ElementId(5)));
}

#[test]
fn insertion_is_atomic_and_preserves_existing_focus() {
    let mut s = scene(vec![leaf(1, Widget::Button("A".into()))]);
    click(&mut s, [20.0, 20.0]);
    assert_eq!(
        s.insert(ElementId(0), leaf(1, Widget::Button("duplicate".into()))),
        Err(UiError::DuplicateId(ElementId(1)))
    );
    assert_eq!(s.focus(), Some(ElementId(1)));
    s.insert(ElementId(0), leaf(2, Widget::Button("B".into())))
        .unwrap();
    s.layout([400.0, 400.0], &mut Metrics);
    assert_eq!(s.focus(), Some(ElementId(1)));
    assert_eq!(
        click(&mut s, [20.0, 68.0]),
        Some(Action::Activated(ElementId(2)))
    );
}

#[test]
fn nonmodal_outside_press_dismisses_without_activating_background() {
    let mut popup = Element::new(10, Widget::Popup { modal: false }).layout(Layout {
        width: Length::Px(100.0),
        height: Length::Px(100.0),
        ..Default::default()
    });
    popup.visible = false;
    let mut s = scene(vec![leaf(1, Widget::Button("A".into())), popup]);
    click(&mut s, [20.0, 20.0]);
    s.open_popup(ElementId(10)).unwrap();
    s.layout([400.0, 400.0], &mut Metrics);
    assert_eq!(
        s.event(Input::PointerDown([20.0, 20.0])).action,
        Some(Action::PopupClosed(ElementId(10)))
    );
    assert!(s.event(Input::PointerUp([20.0, 20.0])).action.is_none());
}

#[test]
fn cursors_follow_controls_capture_and_disabled_state() {
    let mut vertical = Slider::new(0.0, 1.0, 0.5, None).unwrap();
    vertical.orientation = Orientation::Vertical;
    let mut s = scene(vec![
        leaf(1, Widget::Button("Button".into())),
        leaf(2, Widget::TextInput(TextEdit::new("Text").unwrap())),
        leaf(3, Widget::Draggable("Drag".into())),
        leaf(4, Widget::Slider(Slider::new(0.0, 1.0, 0.5, None).unwrap())),
        leaf(5, Widget::Slider(vertical)),
    ]);
    assert_eq!(s.cursor(), CursorShape::Default);
    for (id, expected) in [
        (1, CursorShape::Pointer),
        (2, CursorShape::Text),
        (3, CursorShape::Grab),
        (4, CursorShape::EwResize),
        (5, CursorShape::NsResize),
    ] {
        let b = s.bounds(ElementId(id)).unwrap();
        s.event(Input::PointerMove([b.x + 20.0, b.y + 20.0]));
        assert_eq!(s.cursor(), expected);
    }
    s.event(Input::PointerDown([20.0, 116.0]));
    assert_eq!(s.cursor(), CursorShape::Grabbing);
    s.event(Input::PointerMove([900.0, 900.0]));
    assert_eq!(s.cursor(), CursorShape::Grabbing);
    s.event(Input::Cancel);
    assert_eq!(s.cursor(), CursorShape::Default);
    s.event(Input::PointerMove([20.0, 20.0]));
    s.set_enabled(ElementId(1), false).unwrap();
    assert_eq!(s.cursor(), CursorShape::NotAllowed);
    s.event(Input::PointerLeave);
    assert_eq!(s.cursor(), CursorShape::Default);
}

#[test]
fn cursor_overrides_and_stationary_geometry_changes_respect_hit_targets() {
    let mut s = scene(vec![
        leaf(1, Widget::Button("Card".into()))
            .children(vec![leaf(2, Widget::Text("Child".into()))]),
        leaf(3, Widget::Panel).cursor(CursorShape::Crosshair),
    ]);
    s.event(Input::PointerMove([20.0, 20.0]));
    assert_eq!(s.cursor(), CursorShape::Pointer);
    s.set_cursor(ElementId(1), Some(CursorShape::Progress))
        .unwrap();
    assert_eq!(s.cursor(), CursorShape::Progress);
    s.set_visible(ElementId(1), false).unwrap();
    s.layout([400.0, 400.0], &mut Metrics);
    assert_eq!(s.cursor(), CursorShape::Crosshair);
    s.set_cursor(ElementId(3), Some(CursorShape::Hidden))
        .unwrap();
    assert_eq!(s.cursor(), CursorShape::Hidden);
    s.set_cursor(ElementId(3), None).unwrap();
    assert_eq!(s.cursor(), CursorShape::Default);
}

#[test]
fn menu_cursor_skips_separators_and_does_not_expose_background_actions() {
    let mut s = scene(vec![
        leaf(
            1,
            Widget::Dropdown {
                label: "Menu".into(),
                selected: None,
                items: vec![
                    MenuItem::option(10, "First"),
                    MenuItem::Separator,
                    MenuItem::Option {
                        id: 20,
                        label: "Disabled".into(),
                        enabled: false,
                    },
                ],
            },
        ),
        leaf(2, Widget::Button("Behind".into())),
    ]);
    click(&mut s, [20.0, 20.0]);
    // The menu starts immediately below the 40px trigger and has 32/10/32px rows.
    for (y, expected) in [
        (50.0, CursorShape::Pointer),
        (77.0, CursorShape::Default),
        (95.0, CursorShape::NotAllowed),
        (140.0, CursorShape::Default),
    ] {
        s.event(Input::PointerMove([20.0, y]));
        assert_eq!(s.cursor(), expected);
    }
}
