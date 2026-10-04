const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');
const source = fs.readFileSync(path.join(__dirname, 'language-preference.js'), 'utf8');
function preference(values = {}, denied = false) {
  const storage = new Map(Object.entries(values));
  const context = vm.createContext({ localStorage: {
    getItem(key) { if (denied) throw new Error('denied'); return storage.get(key) ?? null; },
    setItem(key, value) { if (denied) throw new Error('denied'); storage.set(key, value); }
  }});
  vm.runInContext(source, context);
  return { read: () => vm.runInContext("siteLanguage.read('legacy')", context),
    save: value => vm.runInContext('siteLanguage.save(' + JSON.stringify(value) + ')', context), storage };
}
test('valid shared choice wins over conflicting legacy', () => {
  for (const language of ['en', 'ja']) assert.equal(preference({'ba0918-language': language, legacy: language === 'en' ? 'ja' : 'en'}).read(), language);
});
test('missing or invalid shared choice reads only valid local legacy without promotion', () => {
  for (const shared of [undefined, '', 'fr']) for (const legacy of ['en', 'ja']) {
    const p = preference({...shared === undefined ? {} : {'ba0918-language': shared}, legacy});
    assert.equal(p.read(), legacy); assert.equal(p.storage.get('ba0918-language'), shared);
  }
});
test('new and invalid choices default to English without saving', () => {
  for (const values of [{}, {legacy: 'fr'}, {'ba0918-language':'JA', legacy:'invalid'}]) {
    const p = preference(values); assert.equal(p.read(), 'en'); assert.deepEqual(Object.fromEntries(p.storage), values);
  }
});
test('only explicit valid choices are persisted in shared storage', () => {
  const p = preference({legacy:'ja'}); p.save('en'); assert.equal(p.storage.get('ba0918-language'),'en');
  p.save('ja'); p.save('fr'); assert.equal(p.storage.get('ba0918-language'),'ja'); assert.equal(p.storage.get('legacy'),'ja');
});
test('storage rejection does not throw or prevent an English fallback', () => {
  const p = preference({}, true); assert.equal(p.read(),'en'); assert.doesNotThrow(()=>p.save('ja'));
});
