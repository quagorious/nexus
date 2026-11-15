# Troubleshooting

[Deutsche Version](../de/troubleshooting.md) · [← Overview](README.md)

Work along the chain: first check whether the try is on
**quagorious.gg**. If it isn't, the problem is before the addon (arcdps or
the uploader). If it is, the problem is in the addon.

## The try isn't on quagorious.gg

- **Is arcdps recording?** Is there a new `.zevtc` file in
  `Documents\Guild Wars 2\addons\arcdps\arcdps.cbtlogs\` after the fight?
  Very short fights may not be saved by arcdps.
- **Is the uploader running?** It sits in the notification area. Is it
  connected (quagorious.gg → Upload → Uploader, "Last used"), and does the
  log folder in its settings point to the right place?
- **Was it a duplicate?** If a squadmate uploaded the same fight first, your
  file isn't sent. The try is still on Quagorious, and with an account it
  shows up in your list.
- **Failed?** The counters on the uploader page show failed logs.

## The try is on Quagorious, but the addon shows nothing

- **Status line** in Nexus → Options → Quagorious:
  - *Not connected*: enter your API key and click **Connect**.
  - *Error: GW2 API: …*: the key is wrong, deleted, or lacks the `account`
    permission.
  - *Error: Session: …*: Quagorious didn't accept the subtoken, for
    example because there's no Quagorious account for this GW2 account yet,
    or the subtoken expired after a year. Connect again with a key.
  - *live feed lost (…), retrying*: the connection dropped. The addon
    reconnects by itself and catches up on missed tries afterwards.
- **Different account?** The addon shows the tries of the GW2 account
  whose key you entered. If you play on a second account, it has to be
  linked to your Quagorious account.
- **All options off?** If *open window*, *auto-copy* and *notification*
  are all off, nothing visible happens. Open the window with
  `ALT+SHIFT+L`.

## Only the Overview card, no other cards

The extra cards need the try's share link, and the addon only gets it for
tries it saw **live** (see [How it works](how-it-works.md#share-link-and-extra-cards)).
Tries from before installing it, or from while the game was closed, only
have the overview card.

The **Your boon** card also only appears if you provide quickness or
alacrity to your subgroup. The **Fight timeline** needs phases or a wipe.

## Pasting doesn't work

- With *auto-copy* off, the card is only copied with **Copy image** or
  `ALT+SHIFT+K`.
- Copy something else after the notification, and the card is gone from the
  clipboard. Just copy it again.
- Linux: `xclip -selection clipboard -t TARGETS -o` shows whether
  `image/png` is on offer.

## The addon doesn't update

Nexus only picks up GitHub releases with a DLL attached and a higher
version. Pre-releases only if they're enabled for the addon in Nexus. The
installed version is in the Nexus addon list. If in doubt, download the
DLL again from the
[latest release](https://github.com/quagorious/nexus/releases/latest).

## Still stuck?

Open an issue on [GitHub](https://github.com/quagorious/nexus/issues),
with the addon version and the status line text. **Never post your API
key, the contents of `settings.json` or share links you don't want public.**
