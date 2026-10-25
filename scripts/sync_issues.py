"""Mirror docs/WORK.md onto GitHub issues: one issue per task, labels for tier and state.

  python scripts/sync_issues.py          create/label/close issues to match the board
  python scripts/sync_issues.py --dry    print what would change

The board stays the source of truth; this only follows it. Active and Up next rows get an open
issue (title = task name), Done rows close theirs. Up next order is the priority: the top three
also get `priority:high`. Agents add progress with `gh issue comment <n> -b "..."`.
"""
import json
import pathlib
import subprocess
import sys

REPO = "itsonlysimbo/NFSPU-Rust"
BOARD = pathlib.Path(__file__).resolve().parent.parent / "docs" / "WORK.md"
LABELS = {
    "tier:strong": "5319e7", "tier:mid": "1d76db", "tier:light": "c5def5", "tier:user": "fbca04",
    "state:active": "0e8a16", "state:up-next": "bfdadc", "priority:high": "d93f0b",
}
DRY = "--dry" in sys.argv


def gh(*args):
    out = subprocess.run(["gh", *args, "-R", REPO], capture_output=True, text=True, encoding="utf-8")
    if out.returncode:
        sys.exit(f"gh {' '.join(args[:2])} failed: {out.stderr.strip()}")
    return out.stdout


def change(*args):
    print("+ gh", " ".join(a if " " not in a else repr(a) for a in args[:4]))
    if not DRY:
        gh(*args)


def board():
    """(section, cells) for every table row under ## Active / ## Up next / ## Done."""
    section, rows = None, []
    for line in BOARD.read_text(encoding="utf-8").splitlines():
        if line.startswith("## "):
            h = line[3:].lower()
            section = "active" if h.startswith("active") else "up-next" if h.startswith("up next") \
                else "done" if h.startswith("done") else None
        elif section and line.startswith("|") and not line.startswith("|---"):
            cells = [c.strip() for c in line.strip("|").split("|")]
            if cells[0] != "Task":
                rows.append((section, cells))
    return rows


def main():
    have = {l["name"] for l in json.loads(gh("label", "list", "--json", "name", "-L", "200"))}
    for name, color in LABELS.items():
        if name not in have:
            change("label", "create", name, "--color", color)
    issues = {i["title"]: i for i in json.loads(
        gh("issue", "list", "--state", "all", "-L", "500", "--json", "number,title,state,labels,body"))}

    up_rank = 0
    for section, cells in board():
        title = cells[0]
        issue = issues.get(title)
        if section == "done":
            if issue and issue["state"] == "OPEN":
                change("issue", "close", str(issue["number"]), "-c", f"Done: {cells[1]}")
            continue
        tier = "tier:" + cells[1].lower()
        want = {f"state:{section}"} | ({tier} if tier in LABELS else set())
        if section == "up-next":
            up_rank += 1
            if up_rank <= 3:
                want.add("priority:high")
        body = (f"Notes: {cells[2]}" if section == "up-next" else
                f"Agent: {cells[2]}  \nBranch: `{cells[3]}`  \nState: {cells[4]}\n\nNext step: {cells[5]}")
        body += "\n\nMirrored from docs/WORK.md by scripts/sync_issues.py; the board is the record."
        if not issue:
            change("issue", "create", "-t", title, "-b", body, *sum((["-l", l] for l in sorted(want)), []))
            continue
        cur = {l["name"] for l in issue["labels"]}
        managed = cur & set(LABELS)
        args = [a for l in sorted(want - managed) for a in ("--add-label", l)]
        args += [a for l in sorted(managed - want) for a in ("--remove-label", l)]
        if issue["state"] == "CLOSED":
            change("issue", "reopen", str(issue["number"]))
        if args or issue["body"].strip() != body.strip():
            change("issue", "edit", str(issue["number"]), "-b", body, *args)


if __name__ == "__main__":
    main()
