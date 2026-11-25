# Posting cards to Discord

[Deutsche Version](../de/discord.md) · [← Using the addon](using-the-addon.md)

The addon can post your cards to a Discord channel by itself, with no bot,
server or hosting involved. All it needs is a **webhook URL** for the
channel. In a **forum channel**, every raid night gets its own post:

```
#raid-logs  (forum channel)
  └─ 1125_Wed                        ← one post per raid night
        Raid night · Wed 2026-11-25   ← summary, updated after every try
        ┌ ✅ Harvest Temple CM                    [card] ┐  one box per boss,
        └ 20:11–21:05  4 wipes    Kill 7:42             ┘  with its latest card
        ┌ ❌ Dhuum CM                             [card] ┐
        └ 21:14–21:40  2 wipes  best 12.4 %             ┘
        ## Harvest Temple CM · 20:11–21:05   ← one heading per boss
        Wipe 1 · 31.0 % · 20:41       [card]
        …
        Kill · 7:42 · 21:03           [card]
        ## Dhuum CM · 21:14–21:40
        …
```

The summary has one box per boss (green for a kill, red otherwise):

- **Click the small card image** on the right of a box to open the boss's
  latest card in Discord's image preview, right at the top of the post.
- **Click the boss name** to jump to its heading. A boss with only one
  try jumps straight to its card.
- The times run from the start of the boss's first try to the end of its
  last one. They're updated after every try. Times and results are in a
  fixed-width font, so they line up from box to box.
- Discord allows 10 boxes per message. From the 11th boss of a night on,
  bosses get a text line above the boxes instead, without an image.

- A raid night runs until **6:00 the next morning**, so a run past
  midnight stays in one post.
- Discord allows only one level of nesting (channel → post). That's why
  the bosses are headings inside the post, and the summary at the top
  links to each one.
- The addon remembers tonight's post on disk. If the game crashes in the
  middle of a run, the next try still goes into the same post.
- If someone deletes the post, the addon starts a new one with the next try.

## 1. In Discord: create the webhook

You need the **Manage Webhooks** permission on the channel. If you don't
have it, ask whoever runs the server to do this step and send you the URL.

1. Create a **forum channel**, e.g. `#raid-logs` (Server → Create Channel →
   *Forum*). A normal text channel works too: everything then goes straight
   into the channel, without a post per night.
2. If the forum has **Require tags when posting** turned on, turn it off.
   The addon doesn't set tags.
3. Open the channel's settings (⚙️) → **Integrations** → **Webhooks** →
   **New Webhook**.
4. Optionally, give it a name and avatar, e.g. "Quagorious". The posts
   show up as "Quagorious" either way.
5. Click **Copy Webhook URL**. It looks like
   `https://discord.com/api/webhooks/1234567890/AbCdEf…`.

> [!WARNING]
> **Treat the webhook URL like a password.** Anyone who has it can post to
> the channel. Don't paste it in chat or in screenshots. If it leaks,
> delete the webhook in Discord and create a new one.

## 2. In game: connect the addon

1. Open **Nexus → Options → Quagorious** and scroll down to **Discord**.
2. Paste the URL into **Webhook URL** (Ctrl+V). The field hides it.
   - If the URL isn't a Discord webhook URL, a red note shows up under the
     field.
3. Click **Check**. The window should show *Discord webhook works*. This
   posts nothing, it only checks the URL.
4. Choose **Post automatically**:

   | Setting | New kill | New wipe |
   |---|---|---|
   | **Off** (default) | — | — |
   | **Kills only** | card posted | only counted in the summary |
   | **Every try** | card posted | card posted |

5. Optionally, turn on **Only post when I'm the commander** (see below).

The settings are saved right away. The webhook URL is stored in the
addon's `settings.json`, and tonight's post in `discord.json` (only Discord
ids, not the URL). Both are in `addons/Quagorious/`.

## 3. During the raid

- After every boss, the card shown in the window goes into tonight's post.
  That's the same card the copy keybind copies, so you choose the card with
  the arrows in the window. For the whole squad, pick **Squad** once. The
  window remembers it.
- The window then shows *Card posted to Discord*, or an error message.
- **Post to Discord** in the window posts the card you're looking at by
  hand, e.g. a wipe while *Kills only* is set, or a different card for the
  same try. It works for tries of the current raid night.
- The **Squad** card and the share link need tries the addon saw live (see
  [Using the addon](using-the-addon.md#the-cards)). For other tries,
  the Overview card is posted.

## Several people with the addon in one squad

A webhook can't see what's already in the channel. If five people post to
the same webhook, every try shows up five times, each in its own post per
night. So:

- **Only one person per squad posts** (or each person uses their own
  channel), or
- everyone who has the URL turns on **Only post when I'm the commander**,
  so only the person with the commander tag posts.

## Privacy

The Squad card shows the names of everyone in the squad. Post it only to a
channel your group is fine with. The addon never pings anyone: mentions in
the posts are turned off.

## When something goes wrong

| Message | Cause and fix |
|---|---|
| *Not a valid webhook URL* | The URL has to start with `https://discord.com/api/webhooks/`. Copy it again from Discord. |
| *Discord HTTP 401* / *404 Unknown Webhook* | The webhook was deleted or its URL changed. Create a new one and paste it. |
| *Discord HTTP 400 … tags* | The forum requires tags. Turn off *Require tags when posting* in the forum's settings. |
| *Discord rate limit* | Too many posts in a short time (e.g. several people on one webhook). The card is skipped. Post it again with **Post to Discord**. |
| *only tries of the current raid night can be posted* | **Post to Discord** only works for tonight's tries. Copy older cards with **Copy image** instead. |
| Every try ends up straight in the channel, without a post | It's a text channel, not a forum channel. That's expected (see step 1). |
