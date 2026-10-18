# Team setup

One person (usually the CAD lead) does steps 1–3 once. Everyone else does step 4.

## 1. Google Cloud OAuth client

cadsync talks to Drive as each student, with their own Google account, so the team needs an
OAuth client.

1. Go to <https://console.cloud.google.com/>, create a project (e.g. `team1234-cadsync`).
2. **APIs & Services › Library**: enable **Google Drive API**.
3. **APIs & Services › OAuth consent screen**:
   - User type **External** (students sign in with personal accounts).
   - Add scope `.../auth/drive`.
   - **Publish the app** (move it out of *Testing*). In Testing mode, sign-ins expire every 7
     days. Published-but-unverified is fine for a team: members see a "Google hasn't verified this
     app" screen once and click *Continue*. Unverified apps are capped at 100 users.
4. **APIs & Services › Credentials › Create credentials › OAuth client ID** → type **Desktop app**.
   Copy the client id and client secret. For desktop apps the secret isn't really secret (PKCE
   protects sign-in), but keep it inside the team anyway.

## 2. Shared drive

- Use the team's shared drive. Every CAD member needs **Content manager** or higher.
- Copy the drive's id from its URL: `drive.google.com/drive/folders/<this part>`. You can paste
  the whole URL into Settings.
- cadsync creates a hidden `.cadsync/` folder in the drive for check-in records. Don't delete it.
- **Don't also sync this drive with Google Drive for desktop** into the vault folder. Two sync
  tools writing the same files is how files get corrupted.

## 3. SolidWorks Document Manager key

Needed so check-ins can see which parts an assembly uses (the broken-mates fix).

- Request a Document Manager key in the SolidWorks Customer Portal (API Support › Document
  Manager key), or through your reseller. FRC sponsorship licenses usually qualify.
- Put it in Settings › *Document Manager license key* on each PC. It is stored only in the local
  config file and passed to the SwRefs helper through its environment.
- Building the helper: see `sidecar/README.md`.

Without the key cadsync still syncs and locks files, but it can't add dependencies automatically
or catch broken references before a check-in.

## 4. Each computer

1. Install cadsync, open **Settings**, enter the client id, secret, shared drive, and the
   Document Manager key.
2. Keep the vault folder at the default **`C:\CAD\<drive name>`**, the same on every machine.
   SolidWorks stores full paths to parts, so identical paths are what keep assemblies opening
   correctly for everyone.
3. Click **Save and sign in with Google**.
4. In **Vault**, tick **Get** on the folders you work on.

## How to work

- **Check out** a file before editing it. Files you haven't checked out are read-only, and
  SolidWorks opens them read-only, so nobody can overwrite anyone.
- When done, go to **My changes**, pick the files, **Review check-in**, fix anything it flags, write
  what changed, and **Check in**. Parts that your assemblies depend on are added automatically.
- Teammates receive a check-in all at once. If a file is open in SolidWorks, the update waits until
  it's closed.
- Rename and move CAD files from inside SolidWorks (or, later, cadsync), not in Explorer.
  Renaming outside SolidWorks breaks references.
