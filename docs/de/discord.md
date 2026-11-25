# Karten auf Discord posten

[English version](../en/discord.md) · [← Das Addon benutzen](using-the-addon.md)

Das Addon kann deine Karten selbst in einen Discord-Kanal posten, ohne Bot,
Server oder Hosting. Es braucht nur eine **Webhook-URL** des Kanals. In
einem **Forum-Kanal** bekommt jeder Raidabend einen eigenen Post:

```
#raid-logs  (Forum-Kanal)
  └─ 1125_Mi                         ← ein Post pro Raidabend
        Raidabend · Mi 25.11.2026     ← Übersicht, nach jedem Try aktualisiert
        ┌ ✅ Harvest Temple CM                      [Karte] ┐  ein Kasten pro Boss,
        └ 20:11–21:05  4 Wipes      Kill 7:42               ┘  mit seiner neuesten Karte
        ┌ ❌ Dhuum CM                               [Karte] ┐
        └ 21:14–21:40  2 Wipes  bestes 12,4 %               ┘
        ## Harvest Temple CM · 20:11–21:05   ← eine Überschrift pro Boss
        Wipe 1 · 31,0 % · 20:41       [Karte]
        …
        Kill · 7:42 · 21:03           [Karte]
        ## Dhuum CM · 21:14–21:40
        …
```

Die Übersicht hat einen Kasten pro Boss (grün bei einem Kill, sonst rot):

- **Klick auf das kleine Kartenbild** rechts im Kasten öffnet die neueste
  Karte des Bosses in Discords Bildvorschau, direkt oben im Post.
- **Klick auf den Bossnamen** springt zu seiner Überschrift. Ein Boss mit
  nur einem Try springt direkt zu seiner Karte.
- Die Zeiten gehen vom Start des ersten Trys bis zum Ende des letzten. Sie
  werden nach jedem Try aktualisiert. Zeiten und Ergebnisse stehen in
  Festbreitenschrift, damit sie von Kasten zu Kasten untereinander stehen.
- Discord erlaubt 10 Kästen pro Nachricht. Ab dem 11. Boss eines Abends
  bekommen Bosse stattdessen eine Textzeile über den Kästen, ohne Bild.

- Ein Raidabend geht bis **6:00 Uhr am nächsten Morgen**, ein Raid über
  Mitternacht bleibt also in einem Post.
- Discord kann nur eine Ebene verschachteln (Kanal → Post). Deshalb sind
  die Bosse Überschriften im Post, und die Übersicht oben verlinkt jede
  davon.
- Das Addon merkt sich den Post des Abends auf der Festplatte. Stürzt das
  Spiel mitten im Raid ab, landet der nächste Try trotzdem im selben Post.
- Löscht jemand den Post, fängt das Addon mit dem nächsten Try einen neuen an.

## 1. In Discord: den Webhook anlegen

Du brauchst dafür die Berechtigung **Webhooks verwalten** im Kanal. Hast du
sie nicht, bitte die Person, die den Server verwaltet, diesen Schritt zu
machen und dir die URL zu schicken.

1. Leg einen **Forum-Kanal** an, z. B. `#raid-logs` (Server → Kanal
   erstellen → *Forum*). Ein normaler Textkanal geht auch: Dann landet
   alles direkt im Kanal, ohne Post pro Abend.
2. Ist im Forum **Tags beim Posten erforderlich** an, schalte das aus. Das
   Addon setzt keine Tags.
3. Kanaleinstellungen (⚙️) → **Integrationen** → **Webhooks** →
   **Neuer Webhook**.
4. Wenn du willst, gib ihm einen Namen und ein Bild, z. B. "Quagorious".
   Die Posts erscheinen so oder so als "Quagorious".
5. Klick auf **Webhook-URL kopieren**. Sie sieht so aus:
   `https://discord.com/api/webhooks/1234567890/AbCdEf…`.

> [!WARNING]
> **Behandle die Webhook-URL wie ein Passwort.** Wer sie hat, kann in den
> Kanal posten. Nicht in den Chat oder in Screenshots einfügen. Ist sie
> doch rausgegangen: Webhook in Discord löschen und einen neuen anlegen.

## 2. Im Spiel: das Addon verbinden

1. Öffne **Nexus → Optionen → Quagorious** und scroll nach unten zu
   **Discord**.
2. Füge die URL bei **Webhook-URL** ein (Strg+V). Das Feld zeigt sie nicht
   an.
   - Ist es keine Discord-Webhook-URL, erscheint darunter ein roter Hinweis.
3. Klick auf **Prüfen**. Im Fenster sollte *Discord-Webhook funktioniert*
   stehen. Dabei wird nichts gepostet, nur die URL geprüft.
4. Wähle **Automatisch posten**:

   | Einstellung | Neuer Kill | Neuer Wipe |
   |---|---|---|
   | **Aus** (Standard) | — | — |
   | **Nur Kills** | Karte wird gepostet | nur in der Übersicht gezählt |
   | **Jeder Try** | Karte wird gepostet | Karte wird gepostet |

5. Wenn du willst, schalte **Nur posten, wenn ich Kommandeur bin** an
   (siehe unten).

Die Einstellungen werden sofort gespeichert. Die Webhook-URL liegt in der
`settings.json` des Addons, der Post des Abends in `discord.json` (nur
Discord-IDs, nicht die URL). Beide sind in `addons/Quagorious/`.

## 3. Im Raid

- Nach jedem Boss geht die Karte, die im Fenster gezeigt wird, in den Post
  des Abends. Das ist dieselbe Karte, die die Kopier-Taste kopiert. Du
  wählst die Karte also mit den Pfeilen im Fenster. Für den ganzen Squad
  wählst du einmal **Squad**, das Fenster merkt es sich.
- Im Fenster steht danach *Karte an Discord gesendet* oder eine
  Fehlermeldung.
- **An Discord** im Fenster postet die Karte, die du gerade siehst, von
  Hand, z. B. einen Wipe bei *Nur Kills* oder eine andere Karte zum selben
  Try. Das geht für Tries des aktuellen Raidabends.
- Die **Squad**-Karte und der Share-Link brauchen Tries, die das Addon live
  gesehen hat (siehe [Das Addon benutzen](using-the-addon.md#die-karten)).
  Bei anderen Tries wird die Übersichtskarte gepostet.

## Mehrere Leute mit dem Addon im Squad

Ein Webhook sieht nicht, was schon im Kanal steht. Posten fünf Leute über
denselben Webhook, erscheint jeder Try fünfmal, jeweils in einem eigenen
Post pro Abend. Deshalb:

- **nur eine Person pro Squad postet** (oder jede Person nimmt ihren
  eigenen Kanal), oder
- alle mit der URL schalten **Nur posten, wenn ich Kommandeur bin** an,
  dann postet nur, wer das Kommandeur-Zeichen hat.

## Datenschutz

Die Squad-Karte zeigt die Namen aller im Squad. Poste sie nur in einen
Kanal, mit dem deine Gruppe einverstanden ist. Das Addon pingt nie jemanden
an: Erwähnungen sind in den Posts abgeschaltet.

## Wenn etwas nicht klappt

| Meldung | Ursache und Lösung |
|---|---|
| *keine gültige Webhook-URL* | Die URL muss mit `https://discord.com/api/webhooks/` anfangen. Kopier sie noch mal aus Discord. |
| *Discord HTTP 401* / *404 Unknown Webhook* | Der Webhook wurde gelöscht oder hat eine neue URL. Leg einen neuen an und füge ihn ein. |
| *Discord HTTP 400 … tags* | Das Forum verlangt Tags. Schalte *Tags beim Posten erforderlich* in den Forum-Einstellungen aus. |
| *Discord rate limit* | Zu viele Posts in kurzer Zeit (z. B. mehrere Leute an einem Webhook). Die Karte wird ausgelassen. Poste sie mit **An Discord** noch mal. |
| *nur Tries des aktuellen Raidabends können gepostet werden* | **An Discord** geht nur für die Tries des Abends. Ältere Karten kopierst du mit **Bild kopieren**. |
| Jeder Try landet direkt im Kanal, ohne Post | Es ist ein Textkanal, kein Forum-Kanal. Das ist so gewollt (siehe Schritt 1). |
