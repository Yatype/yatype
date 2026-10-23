#!/usr/bin/env python3
"""Разворачивает словарь Hunspell (.dic + .aff) во все формы слов.

Поддерживает то, что нужно для ru_RU (А. Лебедев): однобуквенные флаги,
только суффиксы (SFX), без вложенных флагов.

    python3 tools/expand_hunspell.py data/hunspell/ru_RU > data/ru_forms.txt
"""

import re
import sys


def load_affixes(path):
    rules = {}  # флаг -> [(strip, add, condition_regex)]
    with open(path, encoding="utf-8") as f:
        for line in f:
            parts = line.split()
            if len(parts) < 5 or parts[0] != "SFX":
                continue
            flag, strip, add, cond = parts[1], parts[2], parts[3], parts[4]
            strip = "" if strip == "0" else strip
            add = "" if add == "0" else add
            rules.setdefault(flag, []).append((strip, add, re.compile(cond + "$")))
    return rules


def forms(word, flags, rules):
    yield word
    for flag in flags:
        for strip, add, cond in rules.get(flag, []):
            if cond.search(word) and word.endswith(strip):
                yield word[: len(word) - len(strip)] + add


def main():
    base = sys.argv[1]
    rules = load_affixes(base + ".aff")
    seen = set()
    with open(base + ".dic", encoding="utf-8") as f:
        next(f)  # первая строка — число слов
        for line in f:
            entry = line.strip()
            if not entry:
                continue
            word, _, flags = entry.partition("/")
            # Аббревиатуры (ЧПУ, ЧК) и имена собственные с заглавной не берём:
            # заглавные слова определитель обрабатывает отдельно.
            if not word[:1].islower():
                continue
            for form in forms(word, flags, rules):
                form = form.lower()
                if form not in seen:
                    seen.add(form)
    for form in sorted(seen):
        print(form)


if __name__ == "__main__":
    main()
