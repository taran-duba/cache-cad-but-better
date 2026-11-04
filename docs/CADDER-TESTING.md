# Testing Permafrost (for the CAD team)

You're helping find out whether Permafrost can replace CacheCAD for our team. You don't need to
build anything or use a terminal: just install **v1.4.0** and follow the steps. Break things on
purpose. A bug you report now is a bug that doesn't cost us a file during build season.

> **Use the test folder, not the real CAD folder.** Some steps lock files, break locks and create
> junk. Ask the CAD lead for the test folder's link before you start.

## What you need

- **Two people on two Windows PCs with SolidWorks**, called **Person A** and **Person B** below.
  Most tests need both of you at the same time (a call or sitting next to each other works).
- A Google account each. Both need **Editor** (folder) or **Content manager** (shared drive)
  access to the test folder.
- A **small test assembly with mates**: an assembly plus 2–3 parts in one folder (for example a
  plate, a shaft and a gear). Use throwaway parts, not real robot CAD.

## Install

1. Download **`Permafrost_1.4.0_x64-setup.exe`** from
   <https://github.com/taran-duba/permafrost/releases/tag/v1.4.0> and run it. (Already have an
   older version? It offers the update itself; click **Update to 1.4.0 · restart**.)
2. Open **Permafrost**, click **Sign in with Google**, and allow Drive access. Google may say
   "Google hasn't verified this app". Click **Advanced**, then **Continue**.
3. Pick the **test folder**: browse to it (or paste its link) and click **Use this folder**.
4. In **Vault**, click **Select all** to download everything, then **Sync now** if nothing shows
   up after 15 seconds.

**Check:**
- [ ] Sign-in works and your name/email shows at the top.
- [ ] The files appear under `C:\CAD\<test folder name>` on **both** PCs (same path on both).
- [ ] The sidebar does **not** say "Reference checking is off".
- [ ] Files you haven't checked out are **read-only** (right-click › Properties).

> Write down every error message exactly as it appears. Screenshot them if you can.

---

## Test 1: Browsing and getting files

1. In **Vault**, open a folder, then use the **Filter this folder** box.
2. Untick **Get** on one folder, then click **Sync now**. Tick it again and sync.
3. Click **Select all** inside a subfolder.

**Check:**
- [ ] Browsing is instant (no waiting on the network).
- [ ] Unticking a folder stops its files from being kept up to date, and ticking it brings them back.
- [ ] **Select all** gets the whole folder including subfolders, even ones you'd unticked before.

## Test 2: Check out, edit and push (the everyday workflow)

**Person A:**
1. Add the test assembly: copy it and its parts into `C:\CAD\<test folder>\Test Assembly\`. Open
   the assembly **from that folder** in SolidWorks, do **Save All**, close it.
2. Go to **My changes**, tick all the new files, click **Review changes to push**.
3. If it lists problems, write them down. If not, click **Push changes**.

**Check:**
- [ ] The review shows no problems, and the push succeeds.
- [ ] **Person B** gets the files at the same paths after a sync, all at once.
- [ ] **History** shows the push with its message.

**Then Person A:**
4. In **Vault**, click **Check out** on one part. Open it with the **Open** button, change
   something (for example a dimension), **Save**, close SolidWorks.
5. **My changes** › **Review changes to push** › **Push changes**.

**Check:**
- [ ] **Open** launches SolidWorks with the right file.
- [ ] Before checking out, the file is read-only; after, it's editable.
- [ ] Person B sees the new version after a sync, and **the assembly's mates are intact**.

## Test 3: Check out takes the referenced parts too

**Person A:** click **Check out** on the **assembly**. Include at least one part from a
**different folder** in the assembly.

**Check:**
- [ ] A message says it checked out the assembly "including N part(s) your assemblies use", and
  the parts (also the one in the other folder) now show as checked out by you.
- [ ] Parts that someone else already had are listed by name and left alone.
- [ ] Edit one part, then push the assembly from **My changes** with **Also release the … files
  you didn't edit** ticked. The parts you didn't edit are released too, and nothing is left
  checked out.

## Test 4: Check out all

**Person A:** go into a folder and click **Check out all**.

**Check:**
- [ ] Every file in the folder (and subfolders) that nobody else holds becomes checked out, plus
  parts its assemblies use from other folders.
- [ ] It tells you how many were checked out and how many a teammate already had.
- [ ] Roughly how long it took, and how many files? (Write it down; it works one file at a time.)

## Test 5: Two people, one file

1. **Person A:** **Check out** a part.
2. **Person B:** within about 15 seconds, the part should show 🔒 with Person A's name. Try to
   **Check out** the same part.
3. **Person B:** click the 🔒 name or **Need it?**.

**Check:**
- [ ] Person B can't check it out and gets a clear message naming Person A.
- [ ] **Copy a message to send them** copies a sensible request.
- [ ] In a browser, Person B opens the test folder in Google Drive and tries to upload a new
  version of that part. Does Drive refuse? (Either answer is useful; just report it.)

## Test 6: Taking over a lock

1. **Person A:** check out a part and make an edit but **don't push**.
2. **Person B:** click the 🔒 name › **Take it over…** › **Yes, take it over**.

**Check:**
- [ ] Person B now has the part checked out, at the last pushed version.
- [ ] Person A's unpushed edit is kept as a separate copy on Person A's PC, not lost.

## Test 7: Pre-push checks

Person A, on the test assembly, one at a time. Each should be **caught** in **Review changes to
push**, with a plain explanation of what to do:

1. A **new part added** to the assembly (it should be added to the push automatically).
2. A part **edited without checking it out**.
3. A part referenced from **outside the vault** (for example one saved on the Desktop).
4. A **missing** part (delete one from disk, keep the reference).
5. **Duplicate file names**: put a part with the same name in two different folders, and keep
   the assembly pointing at the correct one by its full path. This should **not** block the push.
6. An assembly that still points to an **old location** (for example it was last saved when the
   files lived somewhere else) but the part's name is unique in the test folder. This should
   **not** block the push, and should show a short note about the old path.
7. If SolidWorks shows an error reading an assembly during the review, the push should **not** be
   blocked: a yellow note names the file and asks you to tick any parts you changed.

**Check:**
- [ ] Cases 1–4 are caught or handled correctly; 5, 6 and 7 don't block you.
- [ ] After a push, the **assembly and its changed parts arrive together** on the other PC.

## Test 8: Safety nets

1. **Read-only:** open a file you haven't checked out and try to save. It should refuse.
2. **File open in SolidWorks:** Person A has an assembly open. Person B pushes a new version.
   Person A's copy should **wait** until they close it, then update.
3. **Crash during a push:** Person A starts a large push and **force-closes Permafrost** partway
   (Task Manager › End task). Reopen it.
4. **Edits without a check-out:** make a change to a file you didn't check out, then sync.
5. **Odd names:** in Drive's website add files named like `A/B`, `CON.SLDPRT` or `part.` (ends
   with a dot), plus two files with the same name in one folder. Sync.
6. **Delete:** **Delete** a file in Permafrost. Then try deleting one that an assembly uses.

**Check:**
- [ ] 1: SolidWorks can't save over it.
- [ ] 2: no corrupted file; the update lands after closing.
- [ ] 3: on reopening, the push finishes or restarts cleanly, with no half-pushed assembly.
- [ ] 4: your edit isn't lost (a "conflict" copy is kept).
- [ ] 5: files show up with safe Windows names (the original name appears with an arrow →
  new name); the app doesn't crash.
- [ ] 6: deleting asks first, it disappears for everyone, and an in-use file warns you.

## Test 9: Rename and move (new in 1.4.0)

Close the test assembly in SolidWorks on **both** PCs first.

1. **Person A:** in **Vault**, click **Rename** on a part the test assembly uses. Change the name
   (keep `.SLDPRT`), click **Rename / move**.
2. **Person B:** click **Sync now**, then open the test assembly in SolidWorks.
3. **Person A:** **Rename** a **folder**, and move a part into a different folder by editing the
   path (for example `Test Assembly/Parts/Gear.SLDPRT`).
4. **Person B:** check out the test assembly. **Person A:** try to rename one of its parts.

**Check:**
- [ ] After step 1, a message says it updated and pushed the assembly that uses it. **History**
  shows "Renamed … to …".
- [ ] After step 2, Person B has the new name only (the old file is gone), and the assembly opens
  with **no missing parts and mates intact**.
- [ ] Step 3 works the same way for a folder and for a move.
- [ ] Step 4 is refused with a message naming Person B, and nothing changes.
- [ ] How long did a rename take? (The first one reads every assembly in the folder, so it's slower.)

## Test 10: Restore from History (new in 1.4.0)

1. **Person A:** push two different edits to a part and the assembly (call them "good" and "oops").
2. In **History**, click the "good" push, then **Restore all files from this push**, and confirm.
3. **Person B:** sync and open the assembly.
4. Try **Restore this file** on a single file from an older push.

**Check:**
- [ ] The assembly and its part both go back to the "good" version, for Person B too, with mates
  intact.
- [ ] History shows a new push "Restored "good" (date)", and the "oops" push is still listed.
- [ ] Restoring a file that a teammate has checked out is refused, and nothing changes.

## Test 11: Updates and stability

- [ ] **Settings › Check for updates** works (it may say you're up to date).
- [ ] Leave Permafrost open for an hour while the other person pushes. It keeps syncing and
  doesn't slow down or crash.
- [ ] Open a second copy of Permafrost. It should refuse or focus the first one.
- [ ] How long does the first sync of a big folder take? How many files?

---

## What to send back

Copy this table and fill it in. For every **Fail**, add what you did, what happened, the exact
error text, and a screenshot. If you can, attach the logs: **Settings › Open logs folder**.

| Test | Pass / Fail | Notes |
| --- | --- | --- |
| Install and sign in | | |
| 1. Browsing and getting files | | |
| 2. Check out, edit and push | | |
| 3. Check out with referenced parts | | |
| 4. Check out all | | |
| 5. Two people, one file | | |
| 6. Taking over a lock | | |
| 7. Pre-push checks (7 cases) | | |
| 8. Safety nets (6 cases) | | |
| 9. Rename and move | | |
| 10. Restore from History | | |
| 11. Updates and stability | | |

**Also tell us:** anything you were used to doing in CacheCAD that you couldn't do here. That
list matters as much as the bugs.

## Known gaps (don't report these as bugs)

- There's no SolidWorks add-in yet: you push, rename and restore from the Permafrost window.
- Rename only works for SolidWorks files on a Windows PC with SolidWorks and reference checking
  on. Don't rename CAD files in Explorer or on the Drive website.
- Mac builds exist but can't read SolidWorks references, so test on Windows.
