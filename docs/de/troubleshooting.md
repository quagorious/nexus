# Fehlersuche

[English version](../en/troubleshooting.md) · [← Übersicht](README.md)

Geh die Kette entlang: Schau zuerst, ob der Try auf **quagorious.gg**
liegt. Wenn nicht, liegt das Problem vor dem Addon (arcdps oder Uploader).
Wenn ja, liegt es am Addon.

## Der Try ist nicht auf quagorious.gg

- **Zeichnet arcdps auf?** Liegt nach dem Kampf eine neue `.zevtc`-Datei in
  `Dokumente\Guild Wars 2\addons\arcdps\arcdps.cbtlogs\`? Sehr kurze Kämpfe
  speichert arcdps eventuell nicht.
- **Läuft der Uploader?** Er sitzt im Infobereich der Taskleiste. Ist er
  verbunden (quagorious.gg → Upload → Uploader, „Last used"), und zeigt der
  Log-Ordner in seinen Einstellungen an die richtige Stelle?
- **War es ein Duplikat?** Hat jemand aus dem Squad denselben Kampf zuerst
  hochgeladen, wird deine Datei nicht geschickt. Der Try liegt trotzdem auf
  Quagorious, und mit Account steht er in deiner Liste.
- **Fehlgeschlagen?** Die Zähler auf der Uploader-Seite zeigen
  fehlgeschlagene Logs.

## Der Try ist auf Quagorious, aber das Addon zeigt nichts

- **Statuszeile** unter Nexus → Optionen → Quagorious:
  - *Nicht verbunden*: API-Key eintragen und **Verbinden** klicken.
  - *Fehler: GW2 API: …*: Der Key ist falsch, gelöscht oder hat die
    Berechtigung `account` nicht.
  - *Fehler: Session: …*: Quagorious hat den Subtoken nicht angenommen, zum
    Beispiel weil es für diesen GW2-Account noch keinen Quagorious-Account
    gibt, oder weil der Subtoken nach einem Jahr abgelaufen ist. Verbinde
    dich neu mit einem Key.
  - *Live-Feed unterbrochen (…), neuer Versuch*: Die Verbindung ist
    abgerissen. Das Addon verbindet sich von selbst neu und holt verpasste
    Tries danach nach.
- **Anderer Account?** Das Addon zeigt die Tries des GW2-Accounts, dessen
  Key du eingetragen hast. Spielst du auf einem Zweitaccount, muss der mit
  deinem Quagorious-Account verknüpft sein.
- **Alle Optionen aus?** Sind *Fenster öffnen*, *automatisch kopieren* und
  *Benachrichtigung* alle aus, passiert nichts Sichtbares. Öffne das
  Fenster mit `ALT+SHIFT+L`.

## Nur die Übersicht, keine weiteren Karten

Die weiteren Karten brauchen den Share-Link des Trys, und den bekommt das
Addon nur für Tries, die es **live** gesehen hat (siehe
[So funktioniert es](how-it-works.md#share-link-und-weitere-karten)).
Tries von vor der Installation oder solche, während das Spiel zu war, haben
nur die Übersicht.

Die Karte **Dein Boon** gibt es außerdem nur, wenn du deiner Untergruppe
Quickness oder Alacrity gibst. Der **Kampfverlauf** braucht Phasen oder
einen Wipe.

## Einfügen klappt nicht

- Ist *automatisch kopieren* aus, wird die Karte nur mit **Bild kopieren**
  oder `ALT+SHIFT+K` kopiert.
- Kopierst du nach der Benachrichtigung etwas anderes, ist die Karte weg.
  Kopier sie einfach nochmal.
- Linux: `xclip -selection clipboard -t TARGETS -o` zeigt, ob `image/png`
  angeboten wird.

## Das Addon aktualisiert sich nicht

Nexus nimmt nur GitHub-Releases mit angehängter DLL und höherer Version,
Vorabversionen nur, wenn sie für das Addon in Nexus eingeschaltet sind. Die
installierte Version steht in der Addon-Liste von Nexus. Im Zweifel lad die
DLL vom [neuesten Release](https://github.com/quagorious/nexus/releases/latest)
nochmal herunter.

## Immer noch nicht?

Mach ein Issue auf [GitHub](https://github.com/quagorious/nexus/issues)
auf, mit der Addon-Version und dem Text der Statuszeile. **Poste niemals
deinen API-Key, den Inhalt von `settings.json` oder Share-Links, die nicht
öffentlich sein sollen.**
