# Set up Quagorious and the uploader

🇩🇪 [Deutsche Version](../de/setup-quagorious.md) · [← Overview](README.md) · Next: [Install the addon →](install-addon.md)

Do this first. The addon only shows tries that are already on Quagorious,
so logs have to get there somehow.

## 1. arcdps

arcdps records your fights. If you already have it, skip this step.

Quagorious explains the install step by step:
[How to install arcdps](https://quagorious.gg/guides/how-to-install-arcdps).

> [!IMPORTANT]
> **With Nexus**, don't copy arcdps in as `d3d11.dll` next to `Gw2-64.exe`.
> Nexus itself uses that file name. Install arcdps through the Nexus addon
> library instead, and Nexus loads it.

After a fight, arcdps saves the log under
`Documents\Guild Wars 2\addons\arcdps\arcdps.cbtlogs\`, in one folder per boss.

## 2. A Quagorious account

The addon needs an account. It only shows **your** tries, and Quagorious
can only tell which ones are yours if it knows your GW2 account.

1. Open [quagorious.gg](https://quagorious.gg) and sign in with a GW2 API
   key.
2. Accounts are in a **closed alpha** for now. If the site asks, briefly
   describe yourself and wait for access.

Uploading in the browser works without an account, but with an hourly
limit. With an account the limit is much higher, and every try you took
part in shows up in your list, including tries a squadmate uploaded.

## 3. Get your logs to Quagorious

There are two ways. Both lead to the same result on the website, and the
addon reacts the same way to both.

### Option A: the Quagorious Uploader (recommended)

A small program that uploads every new log a few seconds after the fight,
so there's nothing to do during the raid.

You'll find it in the app under **Upload → Uploader**
([quagorious.gg/app/uploader](https://quagorious.gg/app/uploader)).

1. **Download** it there: "Download Quagorious Uploader for Windows". For
   Linux the same page offers an AppImage, a `.deb` and an `.rpm`.
2. **Start** `Quagorious-Uploader.exe`. No installation needed. Windows
   doesn't know the publisher and may warn you on first start. To start it
   anyway: "More info" → "Run anyway". What the program does exactly is on
   [What does the uploader do?](https://quagorious.gg/uploader-info).
3. In the settings window, click **"Connect with Quagorious"** and confirm
   the code shown on the website. The uploader gets its own upload token
   for this. It never sees your GW2 API key.
4. **Check the log folder.** It's pre-filled. Otherwise "Search for me"
   finds it (including Wine, Proton, Lutris and Bottles folders on Linux).
5. By default the uploader sends **all your old logs, newest first**, and
   **starts with Windows**. You can change both in the settings. After that
   it sits in the notification area and uploads every new log a few seconds
   after the fight.

Before uploading, the uploader asks whether the fight is already on
Quagorious. If a squadmate was faster, your file isn't sent at all.

The uploader page lists every **connected uploader** with name, creation
date and last use. **Disconnecting** one there stops it uploading at once,
for example on an old or lost computer. Under "Token for the command line",
the same page also offers a token for uploading without the program.

> [!NOTE]
> The uploader and the addon are **separate connections**. The uploader
> connects with a code and can only upload. The addon connects with an API
> key and can only read. Disconnecting one doesn't affect the other.

### Option B: upload in the browser

No program needed. On the same page, under **"Upload logs in the browser"**:

1. Drag in your logs, or the whole `arcdps.cbtlogs` folder with all its
   subfolders ("Choose files" / "Choose folder" works too). The newest logs
   go first.
2. **Keep the tab open** until everything is through. Tries someone already
   uploaded are skipped.

The counters below show how far it is: *Parsed*, *In queue*, *Reserved*,
*Duplicate / empty* and *Failed*.

Without an account there's also the public upload page,
[quagorious.gg/upload](https://quagorious.gg/upload). More on uploading and
sharing: [Upload and share arcdps logs](https://quagorious.gg/guides/upload-and-share-arcdps-logs).

## Check

Do a fight (the training golem counts too) and check whether the try shows
up on quagorious.gg. If it does, the first half works.

Next: **[Install and connect the addon →](install-addon.md)**
