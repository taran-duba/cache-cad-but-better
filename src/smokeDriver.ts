// The CI smoke test's UI driver. It only loads when Rust says this is a smoke run
// (CADSYNC_SMOKE set). It clicks through the real screens against the demo drive and
// reports each step back, so a broken screen or button fails CI on macOS and Windows.
import { api } from "./api";

type Step = { name: string; ok: boolean; detail?: string };

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function waitFor<T>(what: string, fn: () => T | null | undefined | false, ms = 20000): Promise<T> {
  const end = Date.now() + ms;
  for (;;) {
    const v = fn();
    if (v) return v;
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`);
    await sleep(100);
  }
}

const all = (sel: string, root: ParentNode = document) => Array.from(root.querySelectorAll<HTMLElement>(sel));
const text = (e: Element) => (e.textContent ?? "").trim();
const button = (label: string, root: ParentNode = document) =>
  all("button", root).find((b) => text(b) === label && !(b as HTMLButtonElement).disabled);
const rowOf = (name: string) =>
  all("tbody tr").find((tr) => !tr.classList.contains("help-row") && text(tr.querySelector("td")!).endsWith(name));

async function click(what: string, find: () => HTMLElement | null | undefined) {
  (await waitFor(what, find)).click();
}

function setValue(el: HTMLInputElement | HTMLSelectElement, value: string) {
  const proto = el instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, "value")!.set!.call(el, value);
  el.dispatchEvent(new Event(el instanceof HTMLSelectElement ? "change" : "input", { bubbles: true }));
}

const goTab = (label: string) =>
  click(`the ${label} tab`, () => all(".sidenav .navitem").find((b) => text(b) === label));

/** Open the Vault tab at the top of the CAD folder, go into the given folders, and wait until `expectRow` is listed. */
async function vaultAt(expectRow: string, ...folders: string[]) {
  await goTab("My changes"); // leaving the Vault resets its folder
  await goTab("Vault");
  for (const f of folders) {
    await click(`folder ${f}`, () => rowOf(f)?.querySelector<HTMLElement>("button.name"));
    await waitFor(`${f} to open`, () => all(".crumb").some((c) => text(c) === f));
  }
  await waitFor(`${expectRow} in the list`, () => rowOf(expectRow));
}

export async function runSmokeUi(): Promise<void> {
  const steps: Step[] = [];
  const step = async (name: string, fn: () => Promise<void>) => {
    try {
      await fn();
      steps.push({ name, ok: true });
    } catch (e) {
      steps.push({ name, ok: false, detail: `${e instanceof Error ? e.message : e} | page: ${text(document.querySelector(".content") ?? document.body).replace(/\s+/g, " ").slice(0, 400)}` });
      throw e;
    }
  };

  try {
    await step("the demo opens on the Vault", async () => {
      await waitFor("the app shell", () => document.querySelector(".topbar") && document.querySelector(".sidenav"), 60000);
      await waitFor("the CAD folder's contents", () => rowOf("2026 Robot") && rowOf("COTS Library"));
    });

    await step("browse into a folder; check out and release a part", async () => {
      await vaultAt("Frame Rail.SLDPRT", "2026 Robot", "Drivetrain");
      await click("Check out on Frame Rail", () => button("Check out", rowOf("Frame Rail.SLDPRT")));
      await click("Undo check-out", () => button("Undo check-out", rowOf("Frame Rail.SLDPRT")));
      await waitFor("the part to be released", () => button("Check out", rowOf("Frame Rail.SLDPRT")));
    });

    await step("a teammate's check-out shows as locked", async () => {
      await vaultAt("Kraken X60.SLDPRT", "COTS Library");
      await waitFor("the lock on Kraken X60", () => button("Need it?", rowOf("Kraken X60.SLDPRT")));
    });

    await step("every other tab renders", async () => {
      for (const t of ["My changes", "History", "Settings"]) {
        await goTab(t);
        await waitFor(`${t} content`, () => text(document.querySelector(".content") ?? document.body).length > 5);
      }
    });

    await step("Settings: make me a CADmin (roles on)", async () => {
      await goTab("Settings");
      await click("Make me a CADmin", () => button("Make me a CADmin"));
      await waitFor("the CADmin pill", () => all(".topbar .pill").some((p) => text(p) === "CADmin"));
      await waitFor("the CADmin list", () => all(".setting-label").some((l) => text(l) === "CADmins"));
    });

    await step("Vault: give a folder its own editors", async () => {
      await vaultAt("Drivetrain", "2026 Robot");
      await waitFor("the Editors column", () => all("thead th").some((t) => text(t) === "Editors"));
      await click("Editors on Drivetrain", () => button("Editors", rowOf("Drivetrain")));
      const input = await waitFor("the editors box", () =>
        document.querySelector<HTMLInputElement>('input[aria-label="Editors of Drivetrain"]'),
      );
      setValue(input, "jordan@demo.team");
      await click("Save", () => button("Save", input.closest("tr") ?? document));
      await waitFor("Drivetrain's editors", () => text(rowOf("Drivetrain") ?? document.body).toLowerCase().includes("jordan"));
      // A CADmin can still check things out in an area they don't own.
      await vaultAt("Frame Rail.SLDPRT", "2026 Robot", "Drivetrain");
      await waitFor("Check out to stay available to the CADmin", () => button("Check out", rowOf("Frame Rail.SLDPRT")));
    });

    await step("Settings: the area is listed and can be removed; roles can be turned off", async () => {
      await goTab("Settings");
      await waitFor("the area in the list", () => all("li").some((l) => text(l).includes("Drivetrain") && text(l).toLowerCase().includes("jordan")));
      const li = all("li").find((l) => text(l).includes("Drivetrain") && text(l).toLowerCase().includes("jordan"))!;
      await click("Remove area", () => button("Remove", li));
      await waitFor("the area to be gone", () => all("p").some((p) => text(p).startsWith("None yet")));
      await click("Turn roles off…", () => all("button.link").find((b) => text(b) === "Turn roles off…"));
      await click("Turn roles off", () => button("Turn roles off"));
      await waitFor("roles to be off", () => button("Make me a CADmin"));
    });

    await step("switch to the other project and back", async () => {
      const trigger = () => document.querySelector<HTMLElement>(".project-trigger");
      const pick = async (name: string) => {
        (await waitFor("the project switcher", trigger)).click();
        const box = await waitFor("the project search box", () => document.querySelector<HTMLInputElement>(".menu-search"));
        // Type a search that matches only the wanted project, then pick the one that's left.
        setValue(box, name.slice(0, 6).toLowerCase());
        await waitFor("the list to filter", () => all("[cmdk-item]").length === 1 && text(all("[cmdk-item]")[0]) === name);
        all("[cmdk-item]")[0].click();
        await waitFor("the menu to close", () => !document.querySelector(".menu-search"));
      };
      await waitFor("the project switcher", () => trigger());
      const first = trigger()!.dataset.value!;
      await pick("Off-season Prototypes");
      await waitFor("the other project's files", () => rowOf("Swerve Prototypes") && !rowOf("2026 Robot"));
      await waitFor("the switcher to show it", () => trigger()?.dataset.value !== first && text(trigger()!).includes("Off-season"));
      await pick("Demo Team Drive");
      await waitFor("the first project's files", () => rowOf("2026 Robot") && !rowOf("Swerve Prototypes"));
    });
  } catch {
    // the failing step is already recorded
  }
  await api.smokeReport(steps).catch(() => {});
}
