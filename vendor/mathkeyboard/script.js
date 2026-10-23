// /math-keyboard/script.js
const input = document.getElementById('mathInput');
const preview = document.getElementById('mathPreview');
const mathMlPreview = document.getElementById('mathMlPreview');
const keyboard = document.getElementById('keyboard');
const statusText = document.getElementById('statusText');

let currentFormat = 'latex';
let history = [input.value];
let historyIndex = 0;
let historyTimer = null;

const keys = [
  {s:'Esc', t:'text', a:'escape'}, {s:'モード切替', t:'text function', a:'mode'}, {s:'基本記号', t:'text', a:'noop'}, {s:'上付き\n下付き', t:'text', a:'noop'}, {s:'演算子', t:'text', a:'noop'}, {s:'関数\n演算', t:'text', a:'noop'}, {s:'行列\nベクトル', t:'text', a:'noop'}, {s:'微積分', t:'text', a:'noop'}, {s:'集合\n論理', t:'text', a:'noop'}, {s:'ギリシャ\n文字', t:'text', a:'noop'}, {s:'Undo', t:'text blue', a:'undo'}, {s:'Redo', t:'text blue', a:'redo'}, {s:'Del', t:'text', a:'delete'},

  {s:'テンプレート', l:'', t:'text function', a:'template'}, {s:'^', l:'^{}', c:'^{}', p:-1, t:'basic'}, {s:'−', l:'[]', c:'-', t:'basic'}, {s:'−', l:'[ ]', c:'\\left[  \\right]', p:-8, t:'basic'}, {s:'±', l:'\\pm', c:'\\pm ', t:'basic'}, {s:'∞', l:'\\infty', c:'\\infty ', t:'basic'}, {s:'≤', l:'\\leq', c:'\\leq ', t:'basic'}, {s:'≥', l:'\\geq', c:'\\geq ', t:'basic'}, {s:'≈', l:'\\approx', c:'\\approx ', t:'basic'}, {s:'≡', l:'\\equiv', c:'\\equiv ', t:'basic'}, {s:'∴', l:'\\therefore', c:'\\therefore ', t:'basic'}, {s:'∵', l:'\\because', c:'\\because ', t:'basic'}, {s:'Back', t:'text', a:'backspace'},

  {s:'分数', l:'a/b', t:'text function', c:'\\frac{}{}', p:-3}, {s:'aᵇ', c:'^{}', p:-1}, {s:'xᵧ', c:'_{}', p:-1}, {s:'xʸ', c:'^{}', p:-1}, {s:'√x', c:'\\sqrt{}', p:-1}, {s:'ⁿ√x', l:'\\sqrt[n]{x}', c:'\\sqrt[]{}', p:-3}, {s:'|x|', c:'\\left|  \\right|', p:-8}, {s:'‖x‖', c:'\\left\\|  \\right\\|', p:-9}, {s:'⟨x,y⟩', c:'\\langle x,y \\rangle'}, {s:'∑', l:'\\sum', c:'\\sum_{}^{}', p:-4}, {s:'∏', l:'\\prod', c:'\\prod_{}^{}', p:-4}, {s:'∐', l:'\\coprod', c:'\\coprod_{}^{}', p:-4}, {s:'Enter', t:'text enter', a:'enter'},

  {s:'根号', l:'√□', t:'text function', c:'\\sqrt{}', p:-1}, {s:'a⁄b', c:'\\frac{}{}', p:-3}, {s:'( )', c:'\\left(  \\right)', p:-8}, {s:'[ ]', c:'\\left[  \\right]', p:-8}, {s:'{ }', c:'\\left\\{  \\right\\}', p:-9}, {s:'⟨ ⟩', c:'\\langle  \\rangle', p:-8}, {s:'⌊ ⌋', c:'\\lfloor  \\rfloor', p:-9}, {s:'⌈ ⌉', c:'\\lceil  \\rceil', p:-8}, {s:'‖ ‖', c:'\\left\\|  \\right\\|', p:-9}, {s:'|', c:'|'}, {s:'/', c:'/'}, {s:'\\', c:'\\backslash '}, {s:'', t:'empty'},

  {s:'積分', l:'∫', t:'text function', c:'\\int_{}^{} ', p:-5}, {s:'∫', l:'\\int', c:'\\int_{}^{} ', p:-5, t:'calculus'}, {s:'∬', l:'\\iint', c:'\\iint ', t:'calculus'}, {s:'∭', l:'\\iiint', c:'\\iiint ', t:'calculus'}, {s:'∮', l:'\\oint', c:'\\oint ', t:'calculus'}, {s:'d/dx', l:'\\frac{d}{dx}', c:'\\frac{d}{dx}', t:'calculus'}, {s:'∂/∂x', l:'\\frac{\\partial}{\\partial x}', c:'\\frac{\\partial}{\\partial x}', t:'calculus'}, {s:'∇', l:'\\nabla', c:'\\nabla ', t:'calculus'}, {s:'△', l:'\\Delta', c:'\\Delta ', t:'calculus'}, {s:'lim', l:'\\lim', c:'\\lim_{} ', p:-2, t:'calculus'}, {s:'sup', l:'\\sup', c:'\\sup_{} ', p:-2, t:'calculus'}, {s:'inf', l:'\\inf', c:'\\inf_{} ', p:-2, t:'calculus'}, {s:'↑', t:'text nav', a:'up'},

  {s:'極限', l:'lim', t:'text function', c:'\\lim_{x \\to \\infty} '}, {s:'lim', l:'x→∞', c:'\\lim_{x \\to \\infty} '}, {s:'sup', l:'x∈A', c:'\\sup_{x \\in A} '}, {s:'inf', l:'x∈A', c:'\\inf_{x \\in A} '}, {s:'Σ', l:'i=1…n', c:'\\sum_{i=1}^{n} '}, {s:'Π', l:'i=1…n', c:'\\prod_{i=1}^{n} '}, {s:'∂', l:'\\partial', c:'\\partial '}, {s:'∈', l:'\\in', c:'\\in '}, {s:'∉', l:'\\notin', c:'\\notin '}, {s:'⊂', l:'\\subset', c:'\\subset '}, {s:'⊆', l:'\\subseteq', c:'\\subseteq '}, {s:'⇒', l:'\\Rightarrow', c:'\\Rightarrow '}, {s:'↓', t:'text nav', a:'down'},

  {s:'Ctrl', t:'text', a:'noop'}, {s:'Alt', t:'text', a:'noop'}, {s:'Cmd', t:'text', a:'noop'}, {s:'スペース（カーソル位置に挿入）', t:'text space', c:' '}, {s:'Option', t:'text', a:'noop'}, {s:'Cmd', t:'text', a:'noop'}, {s:'変換\nコピー', t:'text nav wide', a:'copy'}, {s:'→', t:'text nav', a:'right'}
];

function renderKeyboard() {
  keys.forEach((key) => {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = `key ${key.t || ''}`;
    button.dataset.action = key.a || 'insert';
    if (key.c !== undefined) button.dataset.code = key.c;
    if (key.p !== undefined) button.dataset.offset = key.p;
    const symbol = document.createElement('span');
    symbol.className = 'key-symbol';
    symbol.innerHTML = (key.s || '').replace(/\n/g, '<br>');
    button.appendChild(symbol);
    if (key.l) {
      const label = document.createElement('span');
      label.className = 'key-label';
      label.textContent = key.l;
      button.appendChild(label);
    }
    keyboard.appendChild(button);
  });
}

function recordHistory(value) {
  if (history[historyIndex] === value) return;
  history = history.slice(0, historyIndex + 1);
  history.push(value);
  historyIndex = history.length - 1;
}

function scheduleHistory() {
  clearTimeout(historyTimer);
  historyTimer = setTimeout(() => recordHistory(input.value), 250);
}

function insertText(text, offset = 0) {
  const start = input.selectionStart;
  const end = input.selectionEnd;
  input.setRangeText(text, start, end, 'end');
  const pos = input.selectionEnd + Number(offset || 0);
  input.setSelectionRange(pos, pos);
  input.focus();
  recordHistory(input.value);
  updatePreview();
}

function moveCursor(delta) {
  const pos = Math.max(0, Math.min(input.value.length, input.selectionStart + delta));
  input.setSelectionRange(pos, pos);
  input.focus();
}

function toMathMLFallback(tex) {
  const escaped = tex.replace(/[&<>]/g, ch => ({'&':'&amp;','<':'&lt;','>':'&gt;'}[ch]));
  return `<math xmlns="http://www.w3.org/1998/Math/MathML">\n  <mtext>${escaped}</mtext>\n</math>`;
}

async function updatePreview() {
  statusText.textContent = currentFormat === 'latex' ? 'LaTeX' : 'MathML';
  if (currentFormat === 'latex') {
    preview.hidden = false;
    mathMlPreview.hidden = true;
    preview.innerHTML = `\\[${input.value || '\\quad'}\\]`;
    if (window.MathJax?.typesetPromise) {
      try { await MathJax.typesetPromise([preview]); }
      catch { preview.textContent = '数式を解釈できませんでした。'; }
    }
  } else {
    preview.hidden = true;
    mathMlPreview.hidden = false;
    let output = toMathMLFallback(input.value);
    try {
      if (window.MathJax?.tex2mml) output = MathJax.tex2mml(input.value, {display: true});
    } catch {}
    mathMlPreview.textContent = output;
  }
}

function undo() {
  if (historyIndex <= 0) return;
  historyIndex--;
  input.value = history[historyIndex];
  input.setSelectionRange(input.value.length, input.value.length);
  updatePreview();
}
function redo() {
  if (historyIndex >= history.length - 1) return;
  historyIndex++;
  input.value = history[historyIndex];
  input.setSelectionRange(input.value.length, input.value.length);
  updatePreview();
}

keyboard.addEventListener('click', async (event) => {
  const button = event.target.closest('.key');
  if (!button) return;
  const action = button.dataset.action;
  if (action === 'insert') insertText(button.dataset.code || '', button.dataset.offset || 0);
  else if (action === 'backspace') {
    if (input.selectionStart !== input.selectionEnd) insertText('');
    else if (input.selectionStart > 0) {
      const p = input.selectionStart;
      input.setRangeText('', p - 1, p, 'end');
      recordHistory(input.value); updatePreview(); input.focus();
    }
  }
  else if (action === 'delete') {
    const p = input.selectionStart;
    if (p < input.value.length) { input.setRangeText('', p, p + 1, 'start'); recordHistory(input.value); updatePreview(); }
    input.focus();
  }
  else if (action === 'left') moveCursor(-1);
  else if (action === 'right') moveCursor(1);
  else if (action === 'up') moveCursor(-10);
  else if (action === 'down') moveCursor(10);
  else if (action === 'enter') insertText('\n');
  else if (action === 'escape') input.blur();
  else if (action === 'undo') undo();
  else if (action === 'redo') redo();
  else if (action === 'copy') {
    const text = currentFormat === 'latex' ? input.value : mathMlPreview.textContent;
    await navigator.clipboard.writeText(text);
    button.querySelector('.key-symbol').textContent = 'コピー済み';
    setTimeout(() => button.querySelector('.key-symbol').innerHTML = '変換<br>コピー', 900);
  }
  else if (action === 'template') insertText('\\begin{aligned}\n  & \\n\\end{aligned}', -15);
});

input.addEventListener('input', () => { scheduleHistory(); updatePreview(); });

document.querySelectorAll('.format-button').forEach(button => {
  button.addEventListener('click', () => {
    document.querySelectorAll('.format-button').forEach(b => b.classList.remove('active'));
    button.classList.add('active');
    currentFormat = button.dataset.format;
    updatePreview();
  });
});

document.getElementById('undoTop').addEventListener('click', undo);
document.getElementById('redoTop').addEventListener('click', redo);
document.getElementById('clearButton').addEventListener('click', () => {
  input.value = '';
  recordHistory('');
  updatePreview();
  input.focus();
});

renderKeyboard();
window.addEventListener('load', updatePreview);
