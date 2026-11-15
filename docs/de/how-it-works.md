# So funktioniert es

[English version](../en/how-it-works.md) · [← Übersicht](README.md)

## Zwei getrennte Verbindungen

| | Quagorious Uploader | Dieses Addon |
|---|---|---|
| Verbindet sich mit | einem Code, den du auf der Website bestätigst | einem GW2-API-Key, der zu einem Subtoken wird |
| Bekommt | ein **Upload-Token** (kann nur hochladen und den Status abfragen) | eine **Sitzung** (kann nur deine Tries lesen) |
| Sieht deinen API-Key | nein | einmal beim Verbinden; er wird nicht gespeichert |
| Widerrufen unter | quagorious.gg → Upload → Uploader | „Trennen" im Addon, oder den API-Key löschen |

Die beiden reden nicht miteinander. Der Uploader schickt Logs an
Quagorious. Das Addon hört nur zu, was Quagorious ausgewertet hat.

## Der Auth-Ablauf des Addons

```
GW2-API-Key ──createsubtoken(account, +1J)──▶ Subtoken          (gespeichert)
Subtoken    ──POST /api/blish/session──────▶ Sitzungstoken (24 h, wird automatisch erneuert)
Sitzung     ──GET  /api/blish/live-token───▶ Live-Token
Live-Token  ──SSE  {liveUrl}/api/blish/events
Sitzung     ──GET  /api/blish/encounters            (deine Tries, neueste zuerst, max. 20)
Sitzung     ──GET  /api/blish/encounters/<id>?locale=de|en
```

1. **Key → Subtoken.** Das Addon holt von der GW2-API einen Subtoken, der
   nur die Berechtigung `account` hat und ein Jahr gilt. Gespeichert wird
   nur der Subtoken (`addons\Quagorious\settings.json`). Der Key wird nicht
   gespeichert.
2. **Subtoken → Sitzung.** Quagorious prüft den Subtoken bei der GW2-API,
   erfährt so, zu welchem GW2-Account er gehört, und stellt eine Sitzung
   für 24 Stunden aus. Das Addon erneuert sie von selbst.
3. **Sitzung → Live-Token → Live-Feed.** Ein Server-Sent-Events-Stream, der
   jeden neuen Try meldet.
4. **Sitzung → Tries.** Die Try-Liste und die Details eines Trys.

Die Endpunkte `/api/blish/…` sind die Modul-API, die Quagorious für Clients
im Spiel anbietet. Die Namen stammen vom Blish-HUD-Modul, das dieselbe API
benutzt.

## Warum das Addon nur deine eigenen Tries sieht

Kurz: **Das Addon sagt nie, wessen Tries es haben will. Das entscheidet der
Server, anhand des GW2-Accounts hinter dem Subtoken.**

Im Einzelnen, aus dem Ablauf oben und dem Code des Addons:

- **Der Subtoken steht für genau einen GW2-Account.** Er wird aus deinem
  API-Key erzeugt, und seine einzige Berechtigung, `account`, ist genau
  die, die den Accountnamen verrät. Quagorious kann bei der GW2-API fragen,
  wem der Token gehört. Mehr kann es damit nicht.
- **Die Sitzung hängt an deinem Quagorious-Account.** Die Antwort auf
  `POST /api/blish/session` enthält neben dem Token `account` (den
  GW2-Accountnamen), `person` (deinen Quagorious-Account) und `accounts`
  (alle damit verknüpften GW2-Accounts). Der Server ordnet den Subtoken
  also deinem Quagorious-Account zu, und deshalb brauchst du einen.
- **Keine Anfrage nennt einen Account oder Filter.** Live-Token, Live-Feed
  und Try-Liste werden alle nur mit der Sitzung abgefragt. Nichts in den
  Anfragen könnte nach fremden Tries fragen. Der Server wählt sie anhand
  der Sitzung aus.
- **Die Try-Liste heißt „deine Tries"**: Die Modul-API liefert die Tries
  der Accounts der Sitzung, mit deiner eigenen Rolle und deinen DPS darin.

Was sich von der Seite des Addons aus nicht beweisen lässt: welche Tries der
Server als deine zählt. Laut dem Quagorious-Guide sind das alle Tries, an
denen du **teilgenommen** hast, auch die, die jemand aus dem Squad
hochgeladen hat. Das Addon kann das nicht prüfen. Es zeigt, was der Server
schickt.

> [!NOTE]
> „Nur deine Tries" heißt nicht „nur deine Daten". Ein Try enthält den
> ganzen Squad: Account- und Charakternamen, DPS, Boons, Downs.
> Squad-Karte und Squad-Fenster zeigen dasselbe wie die Seite des Trys auf
> der Website, die jeder mit dem Link öffnen kann.

## Share-Link und weitere Karten

Jedes Live-Ereignis enthält den **Share-Token** des Trys, denselben wie im
Web-Link `https://quagorious.gg/app/encounters/<token>`. Damit holt das
Addon die volle Auswertung des Trys (Phasen, DPS und Boons jedes Spielers),
die die Modul-API nicht anbietet. Kampfverlauf, Dein Boon und Squad
brauchen sie.

Die Try-Liste der Modul-API enthält keine Share-Tokens. Die weiteren Karten
gibt es deshalb nur für Tries, die das Addon **live** gesehen hat. Es
speichert die Tokens in `addons\Quagorious\tokens.json` (die neuesten
2000), damit sie einen Neustart überstehen.

## Was das Addon speichert

Alles in `Guild Wars 2\addons\Quagorious\`:

| Datei | Inhalt |
|---|---|
| `settings.json` | Subtoken, Sprache, Optionen |
| `tokens.json` | Try-ID → Share-Token |
| `gw2_specs.json`, `icons\` | Spec-Icons von der GW2-API |
| `bosses\` | Boss-Bilder |

Das Addon schreibt weder Tokens noch die rohen Live-Ereignisse ins Log.

## Welche Server es kontaktiert

- `api.guildwars2.com` und `render.guildwars2.com`: Subtoken erzeugen,
  Spec-Icons, ein Teil der Boss-Bilder
- `quagorious.gg` und der dort genannte Live-Server (`live.quagorious.gg`):
  Sitzung, Live-Feed, Tries, volle Auswertung
- `i.imgur.com`: der Rest der Boss-Bilder (dieselbe Tabelle, die
  quagorious.gg benutzt). Sie werden einmal geladen und dann in `bosses\`
  aufbewahrt
- `github.com`: Update-Prüfung, durch Nexus, nicht durch das Addon

Mehr Hintergründe und die Gründe für diese Entscheidungen stehen in den
[ADRs](../ADR/README.md) (Englisch), besonders
[0002 Subtoken](../ADR/0002-auth-store-only-a-subtoken.md),
[0003 Live-Feed](../ADR/0003-live-feed-over-sse.md) und
[0006 Share-Token](../ADR/0006-full-analysis-via-share-token.md).
