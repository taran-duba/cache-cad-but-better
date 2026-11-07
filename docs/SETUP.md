# Setup

For students, using Permafrost is:

1. Install Permafrost.
2. Click **Sign in with Google**, and allow it to access Google Drive.
3. Pick your team's CAD folder: browse to it and click **Use this folder**, or paste a link to it.
   It can be a whole shared drive, a folder inside one, a folder in My Drive, or one shared with
   you. Only that folder syncs.
4. Wait for the first sync. Every PC gets the whole CAD folder and keeps it up to date; there's
   nothing to tick. Files you haven't checked out are read-only.
5. One person (the CADmin) turns on roles: **Settings › Make me a CADmin**, then in **Vault**
   clicks **Editors** on each subsystem's folder (or assembly) and lists who may edit it. Set
   **Editors** on the top-level assembly to nobody so only CADmins change how subsystems fit
   together. Skip this step and anyone can check out anything.

Two people do a one-time step first.

## Whoever builds the app (once, ever)

Permafrost needs a Google "OAuth client" so the Sign in with Google button works. You create it
once and it gets built into the app, so nobody else ever sees it.

1. Go to <https://console.cloud.google.com/> and create a project, e.g. `team1234-permafrost`.
2. **APIs & Services › Library**: enable the **Google Drive API**.
3. **APIs & Services › OAuth consent screen**:
   - User type **External**, since students sign in with personal accounts.
   - Add the scope `.../auth/drive`.
   - Click **Publish app** to move it out of *Testing*. In Testing mode, sign-ins expire every 7
     days. Published but unverified is fine for a team: students see "Google hasn't verified
     this app" once and click *Continue*. Unverified apps are capped at 100 users.
4. **APIs & Services › Credentials › Create credentials › OAuth client ID**, type **Desktop app**.
   Click **Download JSON**.
5. Save that file as **`src-tauri/google-oauth.json`** and build. It's gitignored, so don't
   commit it.
   - For CI builds, add the client id and secret as repository secrets
     `CADSYNC_GOOGLE_CLIENT_ID` and `CADSYNC_GOOGLE_CLIENT_SECRET` instead.
   - A build without either still works, but only in demo mode.

For a desktop app the "secret" isn't really secret; Google's sign-in protects it with PKCE.

## The CAD lead (once per team)

- **The CAD folder:** it can be anywhere in Drive, e.g. a `CacheCAD` folder inside a bigger shared
  drive. Give every CAD member edit access to that folder: **Content manager** or higher on a
  shared drive, or **Editor** on a folder. Permafrost only ever reads and writes inside it. It's
  also fine to use a whole shared drive. Add every CAD member as **Content manager** or
  higher. Permafrost creates a hidden `.permafrost/` folder there for check-in records and team
  settings (push records). Don't delete it. (Drives set up when the app was called cadsync have a `.cadsync/`
  folder instead; Permafrost keeps using it.)
- **Reference checking:** Permafrost reads which parts each assembly uses so pushes can add
  them and catch broken references, check-outs can take an assembly's parts along, and renames
  can update every assembly that uses a file. It asks SolidWorks itself, so there's nothing to set up, but
  SolidWorks must be installed on each PC that pushes files. If SolidWorks isn't open,
  Permafrost starts it in the background (the first review can take a minute) and closes
  it afterwards. No license key is involved. It's on by default; untick **Settings › SolidWorks
  reference checking** on a PC to turn it off there (pushes then won't add an assembly's parts
  automatically, and SolidWorks files can't be renamed from that PC).

## Good to know

- **Vault folder:** files go to `C:\CAD\<folder name>` on every computer. Keep it that way:
  SolidWorks remembers full paths, and identical paths are what keep assemblies opening for
  everyone. You can change it under Settings › Advanced, but change it on every PC.
- **Don't also run Google Drive for desktop** on the same Drive folder into that folder. Two
  sync tools writing the same files is how files get corrupted.

## How to work

- **Check out** a file before editing it. Files you haven't checked out are read-only, and
  SolidWorks opens them read-only, so nobody overwrites anyone. Checking out an assembly also
  checks out every part it uses. **Check out all** checks out a whole folder at once.
- **Open** opens a file in SolidWorks straight from Permafrost.
- When you're done, go to **My changes**, pick your files, click **Review changes to push**, fix
  anything it flags, write what changed, and click **Push changes**. Parts your assemblies use are
  added automatically.
- **Push or undo before you leave the shop.** A forgotten check-out blocks everyone else.
  Check-outs older than a day are highlighted as "forgotten?".
- **Need a file someone else has?** Click their 🔒 name, or **Need it?**. First ask them to push
  it (there's a button that copies a message for you). If you can't reach them, **Take it
  over** breaks their lock and checks it out to you. Their edits that weren't pushed stay
  only on their PC, as a separate copy.
- Teammates get a push all at once. If they have the file open in SolidWorks, the update
  waits until they close it.
- **Rename and move files with Permafrost's Rename button**, not in Explorer or on the Drive
  website. Permafrost updates every assembly and drawing that uses the file and pushes it all
  together. Close the files in SolidWorks first; it won't run while a teammate has any of them
  checked out.
- **Made a mistake?** In **History**, open a push and click **Restore this file** or **Restore all
  files from this push**. It's pushed as a new version, so nothing in History is lost.
