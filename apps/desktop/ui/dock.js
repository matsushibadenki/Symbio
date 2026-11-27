// Adapted from ATOM's native WebView docking controller. Rust owns the layout tree.
import {el} from './dom.js';
import {t} from './i18n.js';
import {icon} from './icons.js';

function nodes(node, kind) {
  if (!node) return [];
  const matches = node.kind === kind ? [node] : [];
  if (node.kind === 'split') matches.push(...nodes(node.first, kind), ...nodes(node.second, kind));
  return matches;
}

function capture(element, event, revision) {
  event.preventDefault();
  element.setPointerCapture(event.pointerId);
  return {element, pointerId: event.pointerId, revision, preview: Promise.resolve(), pending: false};
}

function release(gesture) {
  if (gesture.element.hasPointerCapture(gesture.pointerId)) {
    gesture.element.releasePointerCapture(gesture.pointerId);
  }
}

// The final drop or resize must follow the last native preview, even when it failed.
function preview(gesture, send) {
  if (gesture.pending) return;
  gesture.pending = true;
  gesture.preview = send().catch(() => {}).finally(() => { gesture.pending = false; });
}

export function createDockController({getLayout, view, native, invoke, action, dockCommand, applyLayout}) {
  let drag = null;
  let split = null;

  function endDrag() {
    if (!drag) return null;
    const gesture = drag;
    drag = null;
    release(gesture);
    gesture.element.classList.remove('dragging');
    return gesture;
  }

  function endSplit() {
    if (!split) return null;
    const gesture = split;
    split = null;
    release(gesture);
    return gesture;
  }

  function panelDrag(gesture, phase, pointer) {
    return invoke('panel_drag', {panel: gesture.panel, phase, pointer, expectedRevision: gesture.revision});
  }

  function resize(id, expectedRevision, phase, pointer = null, ratio = null) {
    return invoke('split_resize', {id, expectedRevision, phase, pointer, ratio});
  }

  function cancelDrag() {
    const gesture = endDrag();
    if (gesture) gesture.preview.then(() => panelDrag(gesture, 'cancel', null)).catch(() => {});
  }

  function cancelSplit() {
    const gesture = endSplit();
    if (gesture) gesture.preview.then(() => resize(gesture.id, gesture.revision, 'cancel')).catch(() => {});
  }

  function renderTabs(panel) {
    const layout = getLayout();
    const group = nodes(layout?.root, 'tabs').find(node => node.panels.includes(panel))
      || layout?.floating_groups.find(node => node.panels.includes(panel));
    const panels = group?.panels || [panel];
    const tabs = el('div', {class: 'tabs', role: 'tablist'});
    for (const [index, name] of panels.entries()) {
      const tab = el('button', {
        class: 'tab', role: 'tab', 'aria-selected': name === (group?.active || panel),
        title: t(name) + ' · ' + t('drag'),
      }, icon(name), el('span', {text: t(name)}));
      tab.onpointerdown = event => {
        if (event.button !== 0 || drag || !layout || !native) return;
        drag = {...capture(tab, event, layout.revision), panel: name, x: event.clientX, y: event.clientY, active: false};
      };
      tab.onpointermove = event => {
        if (drag?.element !== tab || event.pointerId !== drag.pointerId) return;
        if (Math.hypot(event.clientX - drag.x, event.clientY - drag.y) > 6) drag.active = true;
        if (drag.active) {
          tab.classList.add('dragging');
          preview(drag, () => panelDrag(drag, 'preview', [event.clientX, event.clientY]));
        }
      };
      tab.onpointerup = event => {
        if (drag?.element !== tab || event.pointerId !== drag.pointerId) return;
        const gesture = endDrag();
        if (gesture.active) {
          const pointer = [event.clientX, event.clientY];
          action(async () => {
            await gesture.preview;
            await panelDrag(gesture, 'drop', pointer);
          });
        } else dockCommand({kind: 'select', panel: name});
      };
      tab.onpointercancel = event => {
        if (drag?.pointerId === event.pointerId && drag.element === tab) cancelDrag();
      };
      tab.onclick = event => {
        if (event.detail === 0) dockCommand({kind: 'select', panel: name});
      };
      tab.onkeydown = event => {
        if (!group || !['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
        event.preventDefault();
        let next = (index + (event.key === 'ArrowLeft' ? -1 : 1) + panels.length) % panels.length;
        if (event.key === 'Home') next = 0;
        if (event.key === 'End') next = panels.length - 1;
        dockCommand({kind: 'select', panel: panels[next]});
      };
      tabs.append(tab);
    }
    return tabs;
  }

  function bindSplitter() {
    const layout = getLayout();
    const entry = nodes(layout?.root, 'split')[Number(view.slice(-1))];
    const element = el('button', {
      class: 'splitter', role: 'separator', 'aria-label': t('resize'),
      'aria-valuemin': 10, 'aria-valuemax': 90, 'aria-valuenow': Math.round((entry?.ratio || 0.5) * 100),
      'aria-orientation': entry?.axis === 'horizontal' ? 'vertical' : 'horizontal',
      'data-axis': entry?.axis || 'horizontal',
    });
    element.onpointerdown = event => {
      if (event.button !== 0 || split || !entry) return;
      split = {...capture(element, event, layout.revision), id: entry.id};
    };
    element.onpointermove = event => {
      if (split?.element !== element || split.pointerId !== event.pointerId) return;
      preview(split, () => resize(split.id, split.revision, 'preview', [event.clientX, event.clientY]));
    };
    element.onpointerup = event => {
      if (split?.element !== element || split.pointerId !== event.pointerId) return;
      const gesture = endSplit();
      const pointer = [event.clientX, event.clientY];
      action(async () => {
        await gesture.preview;
        try {
          applyLayout(await resize(gesture.id, gesture.revision, 'commit', pointer));
        } catch (error) {
          await resize(gesture.id, gesture.revision, 'cancel').catch(() => {});
          throw error;
        }
      });
    };
    element.onpointercancel = event => {
      if (split?.pointerId === event.pointerId && split.element === element) cancelSplit();
    };
    element.onkeydown = event => {
      if (!entry || !['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(event.key)) return;
      event.preventDefault();
      const delta = ['ArrowLeft', 'ArrowUp'].includes(event.key) ? -0.02 : 0.02;
      const ratio = Math.max(0.1, Math.min(0.9, entry.ratio + delta));
      action(async () => applyLayout(await resize(entry.id, layout.revision, 'commit', null, ratio)));
    };
    return element;
  }

  function bindDock() {
    const cancel = () => { cancelDrag(); cancelSplit(); };
    document.addEventListener('keydown', event => { if (event.key === 'Escape') cancel(); });
    window.addEventListener('blur', cancel);
  }

  return {bindDock, bindSplitter, renderTabs};
}
