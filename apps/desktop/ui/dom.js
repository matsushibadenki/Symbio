export function el(tag, attributes = {}, ...children) {
  const element = document.createElement(tag);
  for (const [key, value] of Object.entries(attributes)) {
    if (key === 'class') element.className = value;
    else if (key === 'text') element.textContent = value;
    else if (key.startsWith('on')) element.addEventListener(key.slice(2), value);
    else if (key === 'value' || key === 'checked') element[key] = value;
    else if (value !== null) element.setAttribute(key, String(value));
  }
  for (const child of children.flat()) {
    if (child != null) element.append(child);
  }
  return element;
}
