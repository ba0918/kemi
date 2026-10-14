"""保存した context の概要と、明示した参照・ブロックだけを読む。入力は変更しない。"""

import argparse
import json
from pathlib import Path
import sys


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate key in context JSON")
        result[key] = value
    return result


def _non_json_number(_value):
    raise ValueError("context contains a non-JSON number")


def _select(entries, key, requested, label):
    if not isinstance(entries, list) or any(
        not isinstance(entry, dict) or not isinstance(entry.get(key), str)
        for entry in entries
    ):
        raise ValueError(f"invalid {label} in context")
    wanted = set(requested)
    if wanted.difference(entry[key] for entry in entries):
        raise ValueError(f"requested {label} is not present in this context")
    return [entry for entry in entries if entry[key] in wanted]


def project(source, paths=(), blocks=(), pages=()):
    if not isinstance(source, dict) or type(source.get("kemi")) is not int or source["kemi"] != 3:
        raise ValueError("expected a version 3 explanation context")
    required = ("review", "diff_display_version", "explanation_revision", "explanation", "references")
    if any(key not in source for key in required) or not isinstance(source["references"], dict):
        raise ValueError("expected an explanation context, not a result or event document")
    if not isinstance(source["review"], str) or not isinstance(source["diff_display_version"], str) or type(source["explanation_revision"]) is not int or source["explanation_revision"] < 0:
        raise ValueError("invalid context identity or revision")
    references = source["references"]
    code = references.get("code")
    registered_pages = references.get("pages")
    selected_code = _select(code, "path", paths, "code path")
    selected_pages = _select(registered_pages, "url", pages, "page")
    explanation = source["explanation"]
    if explanation is None:
        sections = []
    elif isinstance(explanation, dict) and isinstance(explanation.get("sections"), list):
        sections = explanation["sections"]
    else:
        raise ValueError("invalid explanation in context")

    outline = []
    selected_sections = []
    found_blocks = set()
    wanted_blocks = set(blocks)
    for section in sections:
        if not isinstance(section, dict) or not isinstance(section.get("id"), str) or not isinstance(section.get("title"), str):
            raise ValueError("invalid section in context")
        all_blocks = section.get("blocks")
        _select(all_blocks, "id", (), "block")
        ids = [block["id"] for block in all_blocks]
        found_blocks.update(ids)
        outline.append({"id": section["id"], "title": section["title"], "blocks": ids})
        selected = [block for block in all_blocks if block["id"] in wanted_blocks]
        if selected:
            selected_sections.append({**section, "blocks": selected})
    if wanted_blocks.difference(found_blocks):
        raise ValueError("requested block is not present in this context")

    return {
        "projection": True,
        "kemi": source["kemi"],
        "review": source["review"],
        "diff_display_version": source["diff_display_version"],
        "explanation_revision": source["explanation_revision"],
        "available": {"code": len(code), "pages": len(registered_pages), "sections": len(sections), "blocks": sum(len(section["blocks"]) for section in sections)},
        "outline": outline,
        "references": {"code": selected_code, "pages": selected_pages},
        "selected_sections": selected_sections,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description="Read a small projection of a retained kemi context; never use it as a full replacement document.")
    parser.add_argument("context", help="saved context JSON, or - for stdin")
    parser.add_argument("--path", action="append", default=[], help="exact code path; repeat to select multiple paths")
    parser.add_argument("--block", action="append", default=[], help="complete explanation block to read; repeat for multiple blocks")
    parser.add_argument("--page", action="append", default=[], help="exact registered page URL; repeat for multiple pages")
    args = parser.parse_args(argv)
    try:
        data = sys.stdin.buffer.read() if args.context == "-" else Path(args.context).read_bytes()
        source = json.loads(data.decode("utf-8"), object_pairs_hook=_unique_object, parse_constant=_non_json_number)
        result = project(source, paths=args.path, blocks=args.block, pages=args.page)
        output = json.dumps(result, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode("utf-8")
    except OSError:
        print("context_summary: cannot read context input", file=sys.stderr)
        return 2
    except (ValueError, TypeError, KeyError) as error:
        print(f"context_summary: {error}", file=sys.stderr)
        return 2
    try:
        sys.stdout.buffer.write(output + b"\n")
        sys.stdout.buffer.flush()
    except OSError:
        print("context_summary: cannot write context projection", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
