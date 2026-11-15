# How it works

🇩🇪 [Deutsche Version](../de/how-it-works.md) · [← Overview](README.md)

## Two separate connections

| | Quagorious Uploader | This addon |
|---|---|---|
| Connects with | a code you confirm on the website | a GW2 API key, turned into a subtoken |
| Gets | an **upload token** (can only upload and check status) | a **session** (can only read your tries) |
| Sees your API key | no | once, when connecting; it isn't stored |
| Revoke at | quagorious.gg → Upload → Uploader | "Disconnect" in the addon, or delete the API key |

The two don't talk to each other. The uploader sends logs to Quagorious.
The addon only listens to what Quagorious has analysed.

## The addon's auth flow

```
GW2 API key ──createsubtoken(account, +1y)──▶ subtoken          (stored)
subtoken    ──POST /api/blish/session──────▶ session token (24h, refreshed automatically)
session     ──GET  /api/blish/live-token───▶ live token
live token  ──SSE  {liveUrl}/api/blish/events
session     ──GET  /api/blish/encounters            (your tries, newest first, max 20)
session     ──GET  /api/blish/encounters/<id>?locale=de|en
```

1. **Key → subtoken.** The addon asks the GW2 API for a subtoken with
   only the `account` permission, valid for one year. Only the subtoken is
   stored (`addons\Quagorious\settings.json`). The key isn't saved.
2. **Subtoken → session.** Quagorious checks the subtoken against the GW2
   API, learns which GW2 account it belongs to, and issues a session that
   lasts 24 hours. The addon renews it by itself.
3. **Session → live token → live feed.** A server-sent-events stream that
   reports every new try.
4. **Session → tries.** The try list and the details of a try.

The `/api/blish/…` endpoints are the module API Quagorious offers for
in-game clients. The names come from the Blish HUD module, which uses the
same API.

## Why the addon only sees your own tries

Short version: **the addon never says whose tries it wants. The server
decides, based on the GW2 account behind the subtoken.**

The details, from the flow above and the addon's code:

- **The subtoken identifies exactly one GW2 account.** It's derived from
  your API key, and its only permission, `account`, is the one that
  reveals the account name. Quagorious can ask the GW2 API who the token
  belongs to. It can't do anything else with it.
- **The session is tied to your Quagorious account.** The answer to
  `POST /api/blish/session` contains, besides the token, `account` (the GW2
  account name), `person` (your Quagorious account) and `accounts` (all GW2
  accounts linked to it). So the server maps the subtoken to your
  Quagorious account, which is why you need one.
- **No request names an account or a filter.** The live token, the live
  feed and the try list are all requested with the session alone. Nothing
  in the requests could ask for someone else's tries. The server picks
  them by session.
- **The try list is "your tries"**: the module API returns the tries of
  the session's accounts, with your own role and DPS in them.

What can't be proven from the addon's side: which tries the server counts
as yours. According to the Quagorious guide, these are all tries you **took
part in**, including the ones a squadmate uploaded. The addon has no way to
check that. It shows whatever the server sends.

> [!NOTE]
> "Only your tries" doesn't mean "only your data". A try contains the
> whole squad: account and character names, DPS, boons, downs. The Squad
> card and the Squad window show the same as the try's page on the website,
> which anyone with the link can open.

## Share link and extra cards

Every live event carries the try's **share token**, the same one as in the
web link `https://quagorious.gg/app/encounters/<token>`. With it the addon
fetches the try's full analysis (phases, every player's DPS and boons),
which the module API doesn't offer. Fight timeline, Your boon and Squad
need it.

The module API's try list doesn't contain share tokens. So the extra cards
only exist for tries the addon saw **live**. It keeps the tokens in
`addons\Quagorious\tokens.json` (newest 2000), so they survive a restart.

## What the addon stores

All in `Guild Wars 2\addons\Quagorious\`:

| File | Contents |
|---|---|
| `settings.json` | subtoken, language, options |
| `tokens.json` | try id → share token |
| `gw2_specs.json`, `icons\` | spec icons from the GW2 API |
| `bosses\` | boss artwork |

The addon doesn't log tokens or the raw live events.

## Which servers it contacts

- `api.guildwars2.com` and `render.guildwars2.com`: creating the subtoken,
  spec icons, part of the boss artwork
- `quagorious.gg` and the live server it names (`live.quagorious.gg`):
  session, live feed, tries, full analysis
- `i.imgur.com`: the rest of the boss artwork (the same table quagorious.gg
  uses). It's downloaded once and then kept in `bosses\`
- `github.com`: update check, done by Nexus, not the addon

More background, and the reasons behind these decisions, are in the
[ADRs](../ADR/README.md), especially
[0002 Subtoken](../ADR/0002-auth-store-only-a-subtoken.md),
[0003 Live feed](../ADR/0003-live-feed-over-sse.md) and
[0006 Share token](../ADR/0006-full-analysis-via-share-token.md).
