# Quagorious und den Uploader einrichten

[English version](../en/setup-quagorious.md) · [← Übersicht](README.md) · Weiter: [Addon installieren →](install-addon.md)

Das kommt zuerst. Das Addon zeigt nur Tries, die schon auf Quagorious
liegen, die Logs müssen also irgendwie dorthin.

## 1. arcdps

arcdps zeichnet deine Kämpfe auf. Wenn du es schon hast, überspring diesen
Schritt.

Quagorious erklärt die Installation Schritt für Schritt:
[arcdps installieren](https://quagorious.gg/de/guides/arcdps-installieren).

> [!IMPORTANT]
> **Mit Nexus** kopierst du arcdps nicht als `d3d11.dll` neben
> `Gw2-64.exe`. Diesen Dateinamen benutzt Nexus selbst. Installier arcdps
> stattdessen über die Addon-Bibliothek von Nexus, dann lädt Nexus es.

Nach einem Kampf speichert arcdps das Log unter
`Dokumente\Guild Wars 2\addons\arcdps\arcdps.cbtlogs\`, ein Ordner pro Boss.

## 2. Ein Quagorious-Account

Das Addon braucht einen Account. Es zeigt nur **deine** Tries, und welche
das sind, weiß Quagorious nur, wenn es deinen GW2-Account kennt.

1. Öffne [quagorious.gg](https://quagorious.gg) und melde dich mit einem
   GW2-API-Key an.
2. Accounts sind gerade in einer **geschlossenen Alpha**. Wenn die Seite
   fragt, beschreib dich kurz und warte auf die Freischaltung.

Hochladen im Browser geht auch ohne Account, dann aber mit einem
stündlichen Limit. Mit Account ist das Limit viel höher, und jeder Try, an
dem du teilgenommen hast, landet in deiner Liste, auch wenn ihn jemand aus
dem Squad hochgeladen hat.

## 3. Die Logs zu Quagorious bringen

Es gibt zwei Wege. Beide führen zum selben Ergebnis auf der Website, und
das Addon reagiert auf beide gleich.

### Weg A: der Quagorious Uploader (empfohlen)

Ein kleines Programm, das jedes neue Log ein paar Sekunden nach dem Kampf
hochlädt. Im Raid musst du also nichts tun.

Du findest es in der App unter **Upload → Uploader**
([quagorious.gg/app/uploader](https://quagorious.gg/app/uploader)).

1. **Herunterladen**: „Download Quagorious Uploader for Windows". Für Linux
   gibt es auf derselben Seite ein AppImage, ein `.deb` und ein `.rpm`.
2. **Starten**: `Quagorious-Uploader.exe`. Keine Installation nötig.
   Windows kennt den Herausgeber nicht und warnt beim ersten Start
   eventuell. So startest du ihn trotzdem: „Weitere Informationen" →
   „Trotzdem ausführen". Was das Programm genau macht, steht unter
   [What does the uploader do?](https://quagorious.gg/uploader-info)
   (Englisch).
3. Klick im Einstellungsfenster auf **„Connect with Quagorious"** und
   bestätige den angezeigten Code auf der Website. Der Uploader bekommt
   dafür ein eigenes Upload-Token. Deinen GW2-API-Key sieht er nie.
4. **Log-Ordner prüfen.** Er ist vorausgefüllt. Sonst findet ihn „Search
   for me" (unter Linux auch in Wine-, Proton-, Lutris- und
   Bottles-Ordnern).
5. Standardmäßig schickt der Uploader **alle alten Logs, die neuesten
   zuerst**, und **startet mit Windows**. Beides kannst du in den
   Einstellungen ändern. Danach sitzt er im Infobereich der Taskleiste und
   lädt jedes neue Log ein paar Sekunden nach dem Kampf hoch.

Vor dem Hochladen fragt der Uploader, ob der Kampf schon auf Quagorious
liegt. War jemand aus dem Squad schneller, wird deine Datei gar nicht erst
geschickt.

Die Uploader-Seite listet alle **verbundenen Uploader** mit Name,
Erstelldatum und letzter Nutzung. **Trennst** du dort einen, hört er sofort
auf hochzuladen, zum Beispiel auf einem alten oder verlorenen Rechner.
Unter „Token for the command line" bietet dieselbe Seite außerdem ein Token
zum Hochladen ohne das Programm.

> [!NOTE]
> Uploader und Addon sind **getrennte Verbindungen**. Der Uploader
> verbindet sich mit einem Code und kann nur hochladen. Das Addon verbindet
> sich mit einem API-Key und kann nur lesen. Trennst du das eine, betrifft
> das das andere nicht.

### Weg B: im Browser hochladen

Kein Programm nötig. Auf derselben Seite, unter **„Upload logs in the
browser"**:

1. Zieh deine Logs hinein, oder den ganzen Ordner `arcdps.cbtlogs` mit
   allen Unterordnern („Choose files" / „Choose folder" geht auch). Die
   neuesten Logs kommen zuerst dran.
2. **Lass den Tab offen**, bis alles durch ist. Tries, die schon jemand
   hochgeladen hat, werden übersprungen.

Die Zähler darunter zeigen den Stand: *Parsed* (ausgewertet), *In queue*
(in der Warteschlange), *Reserved* (reserviert), *Duplicate / empty*
(doppelt oder leer) und *Failed* (fehlgeschlagen).

Ohne Account gibt es auch die öffentliche Upload-Seite
[quagorious.gg/upload](https://quagorious.gg/upload). Mehr zum Hochladen und
Teilen: [arcdps-Logs hochladen und teilen](https://quagorious.gg/de/guides/arcdps-logs-hochladen-teilen).

## Prüfen

Mach einen Kampf (der Trainingsgolem zählt auch) und schau, ob der Try auf
quagorious.gg auftaucht. Wenn ja, läuft die erste Hälfte.

Weiter: **[Das Addon installieren und verbinden →](install-addon.md)**
