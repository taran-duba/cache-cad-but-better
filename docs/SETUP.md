# Setup

For students, using cadsync is:

1. Install cadsync.
2. Click **Sign in with Google**, and allow it to access Google Drive.
3. Pick the team's shared drive. If you're only in one, it's picked for you.
4. In **Vault**, tick **Get** on the folders you work on.

Two people do a one-time step first.

## Whoever builds the app (once, ever)

cadsync needs a Google "OAuth client" so the Sign in with Google button works. You create it
once and it gets built into the app, so nobody else ever sees it.

1. Go to <https://console.cloud.google.com/> and create a project, e.g. `team1234-cadsync`.
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

- **Shared drive:** add every CAD member to the team's shared drive as **Content manager** or
  higher. cadsync creates a hidden `.cadsync/` folder there for check-in records and team
  settings. Don't delete it.
- **SolidWorks key:** request a Document Manager key from the SolidWorks Customer Portal (API
  Support › Document Manager key), or through your reseller. FRC sponsorship licenses usually
  qualify. Then in cadsync go to **Settings › SolidWorks reference checking**, paste it, and click
  **Save**. It's stored in the shared drive, so every member's app picks it up automatically.
  Without it, cadsync still syncs and locks files, but it can't add an assembly's parts to a
  check-in automatically or catch broken references.

## Good to know

- **Vault folder:** files go to `C:\CAD\<drive name>` on every computer. Keep it that way:
  SolidWorks remembers full paths, and identical paths are what keep assemblies opening for
  everyone. You can change it under Settings › Advanced, but change it on every PC.
- **Don't also run Google Drive for desktop** on the same shared drive into that folder. Two
  sync tools writing the same files is how files get corrupted.

## How to work

- **Check out** a file before editing it. Files you haven't checked out are read-only, and
  SolidWorks opens them read-only, so nobody overwrites anyone.
- When you're done, go to **My changes**, pick your files, click **Review check-in**, fix anything
  it flags, write what changed, and click **Check in**. Parts your assemblies use are added
  automatically.
- **Check in or undo before you leave the shop.** A forgotten check-out blocks everyone else.
  Check-outs older than a day are highlighted as "forgotten?".
- **Need a file someone else has?** Click their 🔒 name, or **Need it?**. First ask them to check
  it in (there's a button that copies a message for you). If you can't reach them, **Take it
  over** breaks their lock and checks it out to you. Their edits that weren't checked in stay
  only on their PC, as a separate copy.
- Teammates get a check-in all at once. If they have the file open in SolidWorks, the update
  waits until they close it.
- Rename and move CAD files from inside SolidWorks, not in Explorer. Renaming outside SolidWorks
  breaks references.
