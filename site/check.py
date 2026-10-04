#!/usr/bin/env python3
"""Check the built landing page, translations, local links and language contract."""
import argparse
import json
import subprocess
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit

SITE = Path(__file__).resolve().parent


class Page(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ids = set()
        self.links = []
        self.translations = set()

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if 'id' in attrs:
            if attrs['id'] in self.ids:
                raise ValueError(f'Duplicate id: {attrs["id"]}')
            self.ids.add(attrs['id'])
        for name, value in attrs.items():
            if name in ('href', 'src'):
                self.links.append(value)
            elif name == 'srcset':
                self.links.extend(item.strip().split()[0] for item in value.split(','))
            elif name.startswith('data-i18n'):
                self.translations.add(value)


def check(out):
    for script in SITE.glob('*.js'):
        subprocess.run(['node', '--check', str(script)], check=True)
    subprocess.run(['node', '--test', str(SITE / 'language-preference.test.cjs')], check=True)
    page = Page()
    text = (out / 'index.html').read_text(encoding='utf-8')
    if '{{' in text or '__KAKOI_' in text:
        raise ValueError('Unresolved build placeholder')
    page.feed(text)
    if page.translations:
        source = out / 'messages.js' if (out / 'messages.js').exists() else SITE / 'language.js'
        messages, _ = json.JSONDecoder().raw_decode(source.read_text(encoding='utf-8').split('const messages = ', 1)[1])
        if messages['en'].keys() != messages['ja'].keys():
            raise ValueError('English and Japanese translation keys differ')
        for language in ('en', 'ja'):
            missing = page.translations - messages[language].keys()
            if missing:
                raise ValueError(f'Missing {language} translations: {sorted(missing)}')
        for key in page.translations:
            for copy in messages.values():
                fragment = Page()
                fragment.feed(copy[key])
                page.links.extend(fragment.links)
                if copy[key].startswith('assets/'):
                    page.links.append(copy[key])
    for link in page.links:
        url = urlsplit(link)
        if url.scheme or url.netloc:
            continue
        if url.path and not (out / unquote(url.path)).is_file():
            raise ValueError(f'Missing local asset/link: {link}')
        if not url.path and url.fragment and unquote(url.fragment) not in page.ids:
            raise ValueError(f'Missing anchor: {link}')
    print(f'Checked {out}: translations, {len(page.links)} links/assets, JavaScript, language contract')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, default=SITE.parent / '_site' if (SITE / 'build.py').exists() else SITE)
    check(parser.parse_args().out)
