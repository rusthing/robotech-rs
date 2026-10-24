#!/usr/bin/env python3
"""把 DTO 注释里的值域标签同步进 PowerDesigner 模型与建表 SQL。

用法：

    python3 sync.py --repo <仓库根目录> --check     # 只报告差异，不改文件
    python3 sync.py --repo <仓库根目录> --write     # 写入 LDM / PDM / create-table-*.sql

标签格式见 robotech-macros/doc/dto-field-check.md：

- `@len 1..=50`        字符串长度，DTO 与模型都写
- `@range -180..=180`  数值区间，DTO 与模型都写
- `@enum JobType default=Manual 0=Manual:手动分派, 1=Cron:cron表达式`
  只写在模型里——DTO 侧字段类型已经是枚举，不重复声明值表

模型侧落点：

- `model/*.ldm`  逻辑模型 `EntityAttribute.Comment`（实体编码是表名去掉 `<模块>_` 前缀）
- `model/*.pdm`  物理模型 `Column.Comment`
- `model/create-table-*.sql` 的 `comment on column ... is '...'`

写入是幂等的：已有的 `@len` / `@range` / `@enum` 行整行替换，散文部分原样保留。
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

TAG_RE = re.compile(r"^\s*@(len|range|enum)\b(.*)$")

SCALAR_TYPE_RE = re.compile(
    r"^(?:Option<\s*)*(?:String|bool|i8|i16|i32|i64|i128|u8|u16|u32|u64|u128|"
    r"f32|f64|U8|U16|U32|U64|U128|Duration)"
)

COLUMN_RE = re.compile(r'<o:(Column|EntityAttribute) Id="(o\d+)">(.*?)</o:\1>', re.S)
TABLE_RE = re.compile(r'<o:Table Id="(o\d+)">(.*?)</o:Table>', re.S)
ENTITY_RE = re.compile(r'<o:Entity Id="(o\d+)">(.*?)</o:Entity>', re.S)
COMMENT_RE = re.compile(r"<a:Comment>(.*?)</a:Comment>", re.S)
CODE_RE = re.compile(r"<a:Code>(.*?)</a:Code>")
NAME_RE = re.compile(r"<a:Name>(.*?)</a:Name>")
SQL_COMMENT_RE = re.compile(r"comment on column (\w+)\.(\w+) is\s*'(.*?)';", re.S)



def read_text(path: pathlib.Path) -> str:
    """统一按 LF 处理；写回时恢复原文件的换行符，避免整文件 diff。"""
    raw = path.read_bytes().decode("utf-8")
    return raw.replace("\r\n", "\n") if "\r\n" in raw else raw


def write_text(path: pathlib.Path, text: str) -> None:
    raw = path.read_bytes().decode("utf-8")
    if "\r\n" in raw:
        text = text.replace("\n", "\r\n")
    path.write_bytes(text.encode("utf-8"))


# ------------------------------------------------------------------ DTO


def snake(name: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def parse_dtos(repo: pathlib.Path) -> dict[str, dict[str, dict]]:
    """{表名: {列名: {'tags': {标签: 值}, 'type': 字段类型}}}"""
    result: dict[str, dict[str, dict]] = {}
    field_re = re.compile(
        r"((?:^[ \t]*///[^\n]*\n|[ \t]*#\[[^\n]*\]\n)*)^[ \t]*pub (\w+): ([^,\n]+),", re.M
    )
    for path in sorted(repo.glob("*-api/src/dto/*.rs")):
        text = path.read_text(encoding="utf-8")
        for m in re.finditer(r"#\[crud_dto[^\]]*\]\s*pub struct (\w+) \{(.*?)\n\}", text, re.S):
            table = snake(m.group(1)[: -len("Dto")])
            cols: dict[str, dict] = {}
            for fm in field_re.finditer(m.group(2)):
                block, field, ftype = fm.group(1), fm.group(2), fm.group(3).strip()
                tags: dict[str, str] = {}
                for line in block.splitlines():
                    tm = TAG_RE.match(line.lstrip("/ "))
                    if tm:
                        tags[tm.group(1)] = tm.group(2).strip()
                cols[field] = {"tags": tags, "type": ftype}
            result.setdefault(table, {}).update(cols)
    return result


# ------------------------------------------------------------------ 字典枚举


def parse_dic_enums(repo: pathlib.Path) -> dict[str, dict]:
    """{枚举名: {'default': str|None, 'variants': [(值, 变体名, 说明)]}}"""
    enums: dict[str, dict] = {}
    for path in sorted(repo.rglob("*.rs")):
        if "/target/" in str(path):
            continue
        text = path.read_text(encoding="utf-8")
        for m in re.finditer(r"#\[dic(?:\(([^)]*)\))?\]", text):
            dm = re.search(r"default\s*=\s*(\w+)", m.group(1) or "")
            em = re.search(r"pub enum (\w+)", text[m.end() :])
            if not em:
                continue
            name = em.group(1)
            brace = text.find("{", m.end() + em.end())
            if brace < 0:
                continue
            depth, i = 0, brace
            while i < len(text):
                if text[i] == "{":
                    depth += 1
                elif text[i] == "}":
                    depth -= 1
                    if depth == 0:
                        break
                i += 1
            variants, value, doc = [], -1, ""
            for line in text[brace + 1 : i].splitlines():
                s = line.strip()
                if s.startswith("///"):
                    text_doc = s[3:].strip()
                    if text_doc and not doc:
                        doc = text_doc
                    continue
                vm = re.match(r"(\w+)\s*(?:=\s*(-?\d+))?\s*,", s)
                if vm:
                    value = int(vm.group(2)) if vm.group(2) is not None else value + 1
                    variants.append((value, vm.group(1), doc))
                    doc = ""
            if variants:
                enums[name] = {"default": dm.group(1) if dm else None, "variants": variants}
    return enums


def enum_tag(name: str, enum: dict) -> str:
    items = []
    for value, ident, doc in enum["variants"]:
        doc = doc.replace(",", "，").replace(":", "：").replace("&", "＆")
        items.append(f"{value}={ident}" + (f":{doc}" if doc else ""))
    head = f"@enum {name}" + (f" default={enum['default']}" if enum["default"] else "")
    return f"{head} {', '.join(items)}"


# ------------------------------------------------------------------ 文本工具


def split_prose(comment: str) -> tuple[list[str], list[str]]:
    prose, tags = [], []
    for line in comment.splitlines():
        if TAG_RE.match(line):
            tags.append(line.strip())
        else:
            prose.append(line)
    while prose and not prose[-1].strip():
        prose.pop()
    while prose and not prose[0].strip():
        prose.pop(0)
    return prose, tags


VALUE_LINE_RE = re.compile(r"^\s*-?\d+\s*[:：=\-]\s*\S")


def drop_value_lines(prose: list[str], tag_lines: list[str]) -> list[str]:
    """写 @enum 时删掉散文里的值表行——值表只留在标签里，避免一处改两处漏。"""
    if not any(line.startswith("@enum") for line in tag_lines):
        return prose
    return [line for line in prose if not VALUE_LINE_RE.match(line)]


def escape_xml(text: str) -> str:
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def unescape_xml(text: str) -> str:
    return text.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")


def wanted_tag_lines(info: dict, enums: dict[str, dict]) -> list[str]:
    tags = info["tags"]
    if "enum" in tags:
        return [tags["enum"]]
    ftype = info["type"]
    base = re.sub(r"^(?:Option<\s*)+", "", ftype)
    base = re.sub(r"\s*>+$", "", base).strip()
    if base in enums:
        return [enum_tag(base, enums[base])]
    if SCALAR_TYPE_RE.match(ftype):
        out = []
        if "len" in tags:
            out.append(f"@len {tags['len']}")
        if "range" in tags:
            out.append(f"@range {tags['range']}")
        return out
    return []


def tag_map(dtos: dict[str, dict[str, dict]], enums: dict[str, dict]) -> dict[str, dict[str, list[str]]]:
    """{表名: {列名: 标签行}}"""
    out: dict[str, dict[str, list[str]]] = {}
    for table, cols in dtos.items():
        for col, info in cols.items():
            lines = wanted_tag_lines(info, enums)
            if lines:
                out.setdefault(table, {})[col] = lines
    return out


def lookup(tags: dict[str, dict[str, list[str]]], owner: str, col: str) -> list[str] | None:
    """模型对象编码可能是 `msg_delivery`（物理）或 `delivery`（逻辑），两种都试。"""
    if owner in tags and col in tags[owner]:
        return tags[owner][col]
    for table, cols in tags.items():
        if col in cols and table.split("_", 1)[-1] == owner:
            return cols[col]
    return None


# ------------------------------------------------------------------ 模型


def model_objects(text: str, obj_re: re.Pattern) -> list[tuple[int, int, str]]:
    owners = []
    for om in obj_re.finditer(text):
        cm = CODE_RE.search(om.group(2))
        if cm:
            owners.append((om.start(), om.end(), cm.group(1)))
    return owners


def collect_model(text: str, obj_re: re.Pattern):
    """[(对象编码, 列编码, 注释原文, 列块区间, 名称)]"""
    owners = model_objects(text, obj_re)
    for cm in COLUMN_RE.finditer(text):
        owner = next((code for s, e, code in owners if s <= cm.start() <= e), None)
        if owner is None:
            continue
        block = cm.group(0)
        code_m = CODE_RE.search(block)
        if not code_m:
            continue
        name_m = NAME_RE.search(block)
        comment_m = COMMENT_RE.search(block)
        yield (
            owner,
            code_m.group(1),
            unescape_xml(comment_m.group(1)) if comment_m else None,
            cm.start(),
            cm.end(),
            unescape_xml(name_m.group(1)) if name_m else "",
        )


def missing_columns(path: pathlib.Path, tags: dict[str, dict[str, list[str]]], obj_re: re.Pattern) -> list[str]:
    """模型里找不到对应列的标签——模型漏了表或列。"""
    text = path.read_text(encoding="utf-8")
    present = {(owner, code) for owner, code, *_ in collect_model(text, obj_re)}
    out = []
    for table, cols in tags.items():
        for col in cols:
            hit = (table, col) in present or any(o == table.split("_", 1)[-1] and c == col for o, c in present)
            if not hit:
                out.append(f"{table}.{col}")
    return out


def check_file(path: pathlib.Path, tags: dict[str, dict[str, list[str]]], obj_re: re.Pattern) -> list[str]:
    text = read_text(path)
    problems = []
    for owner, code, comment, *_ in collect_model(text, obj_re):
        want = lookup(tags, owner, code)
        if not want:
            continue
        _, have = split_prose(comment or "")
        if sorted(have) != sorted(want):
            detail = "缺" if not have else ("多" if not set(want) & set(have) else "不一致")
            problems.append(f"{owner}.{code}: {detail} want={want} have={have}")
    return problems


def write_model(path: pathlib.Path, tags: dict[str, dict[str, list[str]]], obj_re: re.Pattern) -> list[str]:
    text = read_text(path)
    edits, changed = [], []
    for owner, code, comment, start, end, name in collect_model(text, obj_re):
        want = lookup(tags, owner, code)
        if not want:
            continue
        prose, have = split_prose(comment or "")
        if sorted(have) == sorted(want) and comment is not None:
            continue
        prose = drop_value_lines(prose, want)
        body = prose if prose else [name or code]
        new_comment = escape_xml("\n".join(body + want))
        block = text[start:end]
        m = COMMENT_RE.search(block)
        if m:
            new_block = block[: m.start()] + f"<a:Comment>{new_comment}</a:Comment>" + block[m.end() :]
        else:
            element = f"<a:Comment>{new_comment}</a:Comment>\n"
            anchor = next(
                (block.find(a) for a in ("<a:History>", "<a:DataType>", "<a:GlobalObjectID/>") if block.find(a) >= 0),
                block.rfind("</o:"),
            )
            line_start = block.rfind("\n", 0, anchor) + 1
            new_block = block[:line_start] + element + block[line_start:]
        edits.append((start, end, new_block))
        changed.append(f"{owner}.{code}")

    for start, end, new_block in sorted(edits, key=lambda x: -x[0]):
        text = text[:start] + new_block + text[end:]
    if changed:
        write_text(path, text)
    return changed


# ------------------------------------------------------------------ SQL


def check_sql(path: pathlib.Path, tags: dict[str, dict[str, list[str]]]) -> list[str]:
    text = read_text(path)
    problems = []
    for m in SQL_COMMENT_RE.finditer(text):
        table, col, body = m.group(1), m.group(2), m.group(3)
        want = lookup(tags, table, col)
        if not want:
            continue
        _, have = split_prose(body)
        if sorted(have) != sorted(want):
            problems.append(f"{table}.{col}: want={want} have={have}")
    return problems


def write_sql(path: pathlib.Path, tags: dict[str, dict[str, list[str]]]) -> list[str]:
    text = read_text(path)
    changed = []

    def repl(m: re.Match) -> str:
        table, col, body = m.group(1), m.group(2), m.group(3)
        want = lookup(tags, table, col)
        if not want:
            return m.group(0)
        prose, have = split_prose(body)
        if sorted(have) == sorted(want):
            return m.group(0)
        prose = drop_value_lines(prose, want)
        new = "\n".join((prose if prose else [col]) + want)
        changed.append(f"{table}.{col}")
        return f"comment on column {table}.{col} is\n'{new}';"

    new_text = SQL_COMMENT_RE.sub(repl, text)
    if changed:
        write_text(path, new_text)
    return changed




MYSQL_COL_RE = re.compile(r"^(\s+)(\w+)(\s+.*?)\s+comment\s+'(.*)'(,?)\s*$")


def _mysql_rewrite(path: pathlib.Path, tags, write: bool) -> list[str]:
    """MySQL 的注释是列定义行内的 `comment '...'`，标签同样追加在散文后面。"""
    text = read_text(path)
    lines = text.split("\n")
    changed = []
    in_create = False
    for i, line in enumerate(lines):
        if re.match(r"^create table\b", line, re.I):
            in_create = True
            continue
        if in_create and line.strip() == ");":
            in_create = False
            continue
        if not in_create:
            continue
        m = MYSQL_COL_RE.match(line)
        if not m:
            continue
        col = m.group(2)
        table = None
        for j in range(i, -1, -1):
            tm = re.match(r"^create table\s+(\w+)", lines[j], re.I)
            if tm:
                table = tm.group(1)
                break
        if not table:
            continue
        want = lookup(tags, table, col)
        if not want:
            continue
        prose, have = split_prose(m.group(4))
        if sorted(have) == sorted(want):
            continue
        prose = drop_value_lines(prose, want)
        new_comment = "\n".join((prose if prose else [col]) + want)
        lines[i] = f"{m.group(1)}{col}{m.group(3)}  comment '{new_comment}'{m.group(5)}"
        changed.append(f"{table}.{col}")
    if changed and write:
        write_text(path, "\n".join(lines))
    return changed


# ------------------------------------------------------------------ 主流程


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", required=True, action="append")
    ap.add_argument("--write", action="store_true")
    ap.add_argument("--check", action="store_true")
    args = ap.parse_args()
    if not args.write and not args.check:
        ap.error("需要 --check 或 --write")

    total_problems = 0
    for repo_str in args.repo:
        repo = pathlib.Path(repo_str).resolve()
        dtos = parse_dtos(repo)
        enums = parse_dic_enums(repo)
        tags = tag_map(dtos, enums)
        n_cols = sum(len(cols) for cols in tags.values())
        n_enum = sum(1 for t in tags.values() for lines in t.values() if lines[0].startswith("@enum"))
        print(f"\n=== {repo.name}: 带标签列 {n_cols}（枚举 {n_enum}），字典枚举 {len(enums)} 个 ===")

        for pdm in sorted(repo.glob("model/*.pdm")):
            if args.write:
                changed = write_model(pdm, tags, TABLE_RE)
                print(f"  [pdm] {pdm.name}: 改 {len(changed)} 列")
            else:
                problems = check_file(pdm, tags, TABLE_RE)
                total_problems += len(problems)
                print(f"  [pdm] {pdm.name}: {len(problems)} 处不一致")
                for line in problems[:8]:
                    print(f"        {line}")

        for ldm in sorted(repo.glob("model/*.ldm")):
            if args.write:
                changed = write_model(ldm, tags, ENTITY_RE)
                print(f"  [ldm] {ldm.name}: 改 {len(changed)} 列")
            else:
                problems = check_file(ldm, tags, ENTITY_RE)
                total_problems += len(problems)
                print(f"  [ldm] {ldm.name}: {len(problems)} 处不一致")
                for line in problems[:8]:
                    print(f"        {line}")

        for pdm in sorted(repo.glob("model/*.pdm")):
            miss = missing_columns(pdm, tags, TABLE_RE)
            if miss:
                print(f"  [模型缺列] {pdm.name}: {', '.join(miss)}")

        for sql in sorted(repo.glob("model/create-table-*.sql")):
            mysql = "mysql" in sql.name
            if args.write:
                changed = _mysql_rewrite(sql, tags, True) if mysql else write_sql(sql, tags)
                print(f"  [sql] {sql.name}: 改 {len(changed)} 列")
            else:
                problems = _mysql_rewrite(sql, tags, False) if mysql else check_sql(sql, tags)
                total_problems += len(problems)
                print(f"  [sql] {sql.name}: {len(problems)} 处不一致")
                for line in problems[:8]:
                    print(f"        {line}")

    return 1 if (args.check and total_problems) else 0


if __name__ == "__main__":
    sys.exit(main())
