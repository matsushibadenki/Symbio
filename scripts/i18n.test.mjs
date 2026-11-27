import assert from 'node:assert/strict';
import test from 'node:test';
import en from '../apps/desktop/ui/locales/en.js';
import ja from '../apps/desktop/ui/locales/ja.js';
import zhCN from '../apps/desktop/ui/locales/zh-CN.js';

globalThis.document = { documentElement: {} };
Object.defineProperty(globalThis, 'navigator', {
  value: { languages: ['fr-CA', 'zh-Hans-CN', 'en-US'] },
  configurable: true,
});
const i18n = await import('../apps/desktop/ui/i18n.js');

test('uses the first supported platform language and applies saved settings', () => {
  assert.equal(i18n.locale, 'zh-CN');
  assert.equal(document.documentElement.lang, 'zh-CN');
  assert.equal(i18n.t('reference_to', { name: 'f' }), '转到单元格“f”');

  i18n.setLocale('ja');
  assert.equal(document.documentElement.lang, 'ja');
  assert.equal(i18n.t('reference_to', { name: 'f' }), 'セル「f」へ移動');

  i18n.setLocale('unsupported');
  assert.equal(i18n.locale, 'en');
  assert.equal(i18n.t('reference_to', { name: 'f' }), 'Go to cell f');
});

test('translation dictionaries have identical keys and placeholders', () => {
  const keys = Object.keys(en).sort();
  const placeholders = text => [...text.matchAll(/\{(\w+)\}/g)].map(match => match[1]).sort();
  for (const dictionary of [ja, zhCN]) {
    assert.deepEqual(Object.keys(dictionary).sort(), keys);
    for (const key of keys) {
      assert.deepEqual(placeholders(dictionary[key]), placeholders(en[key]), key);
    }
  }
});
