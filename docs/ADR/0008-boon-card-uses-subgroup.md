# 0008. The boon card lists your subgroup

- Status: Accepted (supersedes the first, provider-based version)
- Date: 2026-11-14

## Context

The first boon card listed every player for whom the analysis named you as
the boon **provider** (`boonGaps.<boon>.provider`). On a Deimos wipe as
alac-heal this gave 6 rows. One player, alone in subgroup 5 with no
alacrity source of their own, was attributed to you at 0.1 % and pulled the
average from 92.5 % down to 74.0 %. On Slothasor, a player in your own
subgroup was attributed to someone else and missing.

In GW2, alacrity and quickness go to your party: same subgroup, same team,
same boons. The analysis has `subgroup` for every player.

## Decision

- The rows are **your subgroup** (`party()`: same `subgroup`, at most four
  others), sorted by uptime, with yourself ("du") last.
- The boon comes from your role: aHeal/aDPS → alacrity, qHeal/qDPS →
  quickness. The provider data is only a fallback, and only within your
  party.
- Uptime is `uptimeWeighted[boon]`. Each row shows the two biggest gap
  causes (≥ 0.5 s) when uptime is below 99.5 %.
- A party member **out of range for at least half the fight** (e.g. a
  kiter) is still listed, but greyed out, marked "nicht im Ø" and left out
  of the average. (This rule was added first for the out-of-party player
  above, and now applies only within your party.)

## Consequences

- The card matches what a support player is responsible for, and the
  average matches the "Ø … bei N Spielern" of the reference design.
- Gap causes are translated as follows: `removed` = vom Gegner entfernt,
  `durationShort` = zu kurz gehalten, `outOfRange` = außer Reichweite,
  `notReached` = nicht erreicht, `providerDown` = Quelle down, `opener`,
  `unknown`. `sourceIdle` ("Quelle zu selten") and `pulseOff` ("Puls aus")
  are guesses.
