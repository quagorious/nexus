# 0013. Posting to Discord through a channel webhook

- Status: Accepted
- Date: 2026-11-25

## Context

Groups want every try's card in a Discord channel without anyone pasting
it, ideally sorted as raid night → boss → tries. It should work for anyone
who installs the addon, with nothing to host.

## Decision

- **Webhook, no bot.** The user pastes a channel webhook URL. The addon
  posts the PNG itself (`multipart/form-data`: `payload_json` + `files[0]`,
  `wait=true`, `allowed_mentions: { parse: [] }`). Only `https://` URLs on
  Discord's hosts are accepted. The URL is a secret: it's never logged, and
  transport errors are reworded so they don't include it.
- **One forum post per raid night.** Discord nests only channel → thread,
  so "night → boss → tries" becomes: a forum post (`thread_name` =
  `MMDD_Weekday`), a summary as its first message, a `## Boss · 20:11–21:05`
  header message per boss (first try's start to last try's end), and the
  card messages. After every try the boss's header and the summary are
  edited (`PATCH …/messages/<id>?thread_id=`). The summary links to each
  header and shows the same time span. Discord has no tables, so each
  summary line starts with one padded code span (time span, wipes, result;
  right-aligned so nothing ends in spaces) that lines up as columns,
  then ✅/❌ and the linked boss name. A boss with a single try links to
  its card message instead of its header. The image itself can't be
  linked durably: attachment URLs are signed and expire (about 24 h).
- **Thumbnails in the summary.** A jump link always scrolls, so the
  summary is one embed per boss (max 10; more bosses get text lines in the
  content): title ✅/❌ + boss (linked), the code span as description,
  green/red colour, and the boss's latest card as `thumbnail`. A click on
  the thumbnail opens Discord's image preview at the top of the post. The
  card is uploaded with the summary edit (`attachments: [{id: 0}]`,
  `attachment://thumb-<try>.png`). Discord lists an image an embed shows
  only in that embed (CDN URL `…/attachments/<channel>/<attachment id>/
  <file>`), not in `attachments`, so the ids to keep are parsed from there
  and sent as `attachments: [{id}]` on the next edit; otherwise Discord
  deletes them. Verified on 2026-11-25. If Discord rejects the embed edit,
  the summary falls back to the plain text lines.
  Verified on 2026-11-25 against a forum channel: the created message's
  `channel_id` is the new post's thread id, and editing the summary works.
- **A raid night ends at 6:00 local time**, so a run past midnight stays in
  one post.
- **State on disk, saved after every step.** Webhooks can't list threads,
  so `discord.json` (`quagorious_api::daylog::DayLog`) keeps the thread,
  summary, header and card message ids. It's written via temp file +
  rename, after every Discord call that changed it. After a game crash the
  next try continues in the same post. The file holds no secret, only ids
  and the webhook's id.
- **Self-healing.** A 404 in tonight's post (deleted by hand) forgets the
  ids, keeps the counts and starts a new post. A text channel (the created
  message's `channel_id` equals the webhook's channel, since Discord
  ignores or rejects `thread_name` there, which isn't documented) switches
  to posting everything flat into the channel.
- **What gets posted:** the focused card variant (the same one the copy
  keybind copies). Options: Off (default) / Kills only (wipes counted in
  the summary only) / Every try, plus "only when I'm the commander". A
  window button posts the shown card by hand (current raid night only).
- **Own thread.** Posting runs on its own thread, so a slow Discord never
  delays the cards. A 429 is retried once if `retry_after` is at most 5 s.

## Consequences

- No hosting, and any group can use it. The person setting it up needs
  "Manage Webhooks" on the channel.
- Several posters on one webhook create duplicate posts: the webhook can't
  read the channel. The commander-only option and the docs are the
  mitigation.
- Tries missed while the game was closed aren't posted (only live and
  newly listed tries count as new). The window button covers single ones.
- Forums with "require tags" reject the posts. `applied_tags` would need
  per-server tag ids, so it's left out for now.
- The Squad card shows other players' names. Posting it is the user's
  choice. The docs say so.
