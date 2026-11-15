# 0010. Spec icons from the GW2 API, boss artwork from the site's table

- Status: Accepted
- Date: 2026-11-14

## Context

The reference cards show profession icons under the squad-DPS bars and boss
artwork in the header.

## Decision

- **Spec icons:** load `/v2/specializations?ids=all` and
  `/v2/professions?ids=all` from the GW2 API once (cached in
  `gw2_specs.json`, refreshed after 14 days). Download each spec's
  `profession_icon_big` on first use into `icons/`. That covers all 36 elite
  specs, including the newest ones (e.g. "Galeshot", which we'd have guessed
  wrong). Icons become a brightness × alpha mask and are tinted with the
  usual profession colour. Without an icon, a three-letter abbreviation is
  drawn.
- **Boss artwork:** quagorious.gg's code has a table of 72 boss ids → image
  URLs (render.guildwars2.com, a few on imgur). We copied it to
  `assets/boss_images.json`. The boss id comes from the live event or the
  full analysis; the module API has none. Images are cached in `bosses/`
  and drawn top-right in the header, fading in from the left and clipped to
  the card's corner.

## Consequences

- No icons are bundled. The first card of a session may take a moment
  longer while icons download.
- New bosses need the table updated (copy it from the site again).
- Tries without a boss id (no share token) get no boss image.
