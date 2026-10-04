const siteLanguage = {
  read(legacyKey) {
    try {
      const shared = localStorage.getItem('ba0918-language');
      if (shared === 'en' || shared === 'ja') return shared;
      const legacy = localStorage.getItem(legacyKey);
      if (legacy === 'en' || legacy === 'ja') return legacy;
    } catch { /* Storage may be disabled; switching still works in this page. */ }
    return 'en';
  },
  save(language) {
    if (language !== 'en' && language !== 'ja') return;
    try { localStorage.setItem('ba0918-language', language); }
    catch { /* Apply the choice even when persistence is unavailable. */ }
  }
};
