"""Discord bridge for the agents' channel.

  python scripts/discord_bridge.py post "<agent>: <text>"   post one message (any agent can call this)
  python scripts/discord_bridge.py read [n]                 print the last n messages (default 20), oldest first
  python scripts/discord_bridge.py listen                    run the bot; the user's messages go to the inbox

The token comes only from the DISCORD_BOT_TOKEN user environment variable (never the repo).
Only messages from OWNER_ID in CHANNEL_ID are taken; everything else is ignored. The inbox is
logs/discord_inbox.md (logs/ is not tracked): one line per message, read by the coordinator.
"""
import datetime
import json
import os
import pathlib
import sys
import urllib.request

OWNER_ID = 95962427379703808
CHANNEL_ID = 1563748992322633849
ROOT = pathlib.Path(__file__).resolve().parent.parent
INBOX = ROOT / "logs" / "discord_inbox.md"


def token():
    t = os.environ.get("DISCORD_BOT_TOKEN")
    if not t and sys.platform == "win32":
        # set after this shell started: read the saved user variable
        import winreg
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, "Environment") as k:
                t = winreg.QueryValueEx(k, "DISCORD_BOT_TOKEN")[0]
        except OSError:
            pass
    if not t:
        sys.exit("DISCORD_BOT_TOKEN is not set")
    return t.strip()


def api(path, data=None):
    req = urllib.request.Request(
        f"https://discord.com/api/v10/channels/{CHANNEL_ID}/{path}",
        data=json.dumps(data).encode() if data is not None else None,
        headers={"Authorization": f"Bot {token()}", "Content-Type": "application/json",
                 "User-Agent": "nfspu-bridge (local, 1.0)"},
    )
    return json.loads(urllib.request.urlopen(req, timeout=20).read() or b"null")


def read(n):
    # every agent posts through the same bot and signs its name, so print the text as written
    for m in reversed(api(f"messages?limit={min(n, 100)}")):
        who = "user" if int(m["author"]["id"]) == OWNER_ID else "agent"
        print(f"[{m['timestamp'][:16].replace('T', ' ')}] {who}: {m['content']}")


def post(text):
    # plain REST call, no gateway needed; Discord caps a message at 2000 characters
    for i in range(0, len(text), 1900):
        api("messages", {"content": text[i:i + 1900], "allowed_mentions": {"parse": []}})


def listen():
    import discord
    intents = discord.Intents.default()
    intents.message_content = True
    client = discord.Client(intents=intents)

    @client.event
    async def on_ready():
        print(f"listening as {client.user} in channel {CHANNEL_ID}", flush=True)

    @client.event
    async def on_message(m):
        if m.author.id != OWNER_ID or m.channel.id != CHANNEL_ID:
            return
        INBOX.parent.mkdir(exist_ok=True)
        stamp = datetime.datetime.now().strftime("%Y-%m-%d %H:%M")
        with INBOX.open("a", encoding="utf-8") as f:
            f.write(f"- [{stamp}] {m.content.replace(chr(10), ' / ')}\n")
        await m.add_reaction("\N{EYES}")

    client.run(token())


if __name__ == "__main__":
    if len(sys.argv) >= 3 and sys.argv[1] == "post":
        post(" ".join(sys.argv[2:]))
    elif sys.argv[1:2] == ["read"]:
        read(int(sys.argv[2]) if len(sys.argv) > 2 else 20)
    elif sys.argv[1:] == ["listen"]:
        listen()
    else:
        sys.exit(__doc__)
