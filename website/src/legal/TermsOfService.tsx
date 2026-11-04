import { ContactLine, LegalLayout, Section, type TocItem } from "./LegalLayout";
import { REPO_URL } from "../lib/release";

const TOC: TocItem[] = [
  { id: "agreement", label: "Agreeing to these terms" },
  { id: "service", label: "What Permafrost is" },
  { id: "accounts", label: "Your Google account" },
  { id: "use", label: "Acceptable use" },
  { id: "content", label: "Your files" },
  { id: "backups", label: "Locks, pushes and backups" },
  { id: "third-party", label: "Other companies' products" },
  { id: "source", label: "Source code" },
  { id: "warranty", label: "No warranty" },
  { id: "liability", label: "Limitation of liability" },
  { id: "ending", label: "Stopping use" },
  { id: "changes", label: "Changes to these terms" },
  { id: "contact", label: "Contact" },
];

export function TermsOfService() {
  return (
    <LegalLayout
      eyebrow="Legal"
      title="Terms of Service"
      toc={TOC}
      summary={
        <>
          <p className="legal-summary-title">The short version</p>
          <ul>
            <li>Permafrost is free. Use it to share your team's CAD files with people who are allowed to see them.</li>
            <li>Your files stay yours. We never receive them.</li>
            <li>The software comes as is. Keep backups of anything important.</li>
          </ul>
        </>
      }
    >
      <Section id="agreement" title="Agreeing to these terms">
        <p>
          These terms apply to the Permafrost desktop app ("Permafrost", "the app") and this website ("the site"), which
          are provided by the Permafrost project ("we", "us"). By downloading, installing or using Permafrost, you agree to
          these terms and to our <a href="/privacy-policy">Privacy Policy</a>. If you don't agree, please don't use the
          app.
        </p>
        <p>
          If you're under 18, please make sure a parent, guardian or team mentor is fine with you using Permafrost and
          these terms.
        </p>
      </Section>

      <Section id="service" title="What Permafrost is">
        <p>
          Permafrost is a free desktop app that syncs SolidWorks files between your computer and a Google Drive shared
          drive or folder, with check-out locks and pushes to help a team avoid overwriting each other's work. It runs
          on your computer and connects directly to Google. We don't operate a server that stores or processes your
          files.
        </p>
        <p>
          We may change, improve or stop working on Permafrost at any time. Versions you've already downloaded keep
          working as long as Google's services allow them to.
        </p>
      </Section>

      <Section id="accounts" title="Your Google account">
        <ul>
          <li>
            You sign in with your own Google account, and you're responsible for keeping it secure. Your use of Google
            Drive is also governed by <a href="https://policies.google.com/terms">Google's Terms of Service</a> and, for
            school or organization accounts, by your administrator's policies.
          </li>
          <li>
            Only use Permafrost with drives and folders you're allowed to access, and follow your team's and school's
            rules for sharing files.
          </li>
          <li>
            You can stop Permafrost's access at any time by signing out in the app or removing it from your Google
            Account at <a href="https://myaccount.google.com/connections">myaccount.google.com/connections</a>.
          </li>
        </ul>
      </Section>

      <Section id="use" title="Acceptable use">
        <p>Please don't use Permafrost to:</p>
        <ul>
          <li>access, change or delete files you don't have permission to;</li>
          <li>share files you don't have the right to share, or that infringe someone else's intellectual property;</li>
          <li>store or spread malware, or anything illegal;</li>
          <li>
            interfere with Google's services, get around their limits, or use Google's APIs in ways their terms don't
            allow;
          </li>
          <li>misrepresent Permafrost as an official product of Google, Dassault Systèmes or FIRST.</li>
        </ul>
      </Section>

      <Section id="content" title="Your files">
        <p>
          You and your team keep all rights to your CAD files and everything else you sync. We don't claim any ownership
          of them and never receive a copy. What happens to files in your Drive is up to you, your team and Google's
          terms.
        </p>
      </Section>

      <Section id="backups" title="Locks, pushes and backups">
        <p>
          Check-out locks, read-only files and reference checks are there to help your team avoid mistakes. They depend on
          Google Drive, SolidWorks and your computers working as expected, so they can't guarantee that files will never
          be lost, changed or broken. For example, someone with edit access can still change files directly in Drive.
        </p>
        <p>
          Keep your own backups of important work, and use Google Drive's version history and trash to recover earlier
          versions when you need to.
        </p>
      </Section>

      <Section id="third-party" title="Other companies' products">
        <p>
          Permafrost works with Google Drive and SolidWorks, and its downloads are served by GitHub. Those products have
          their own terms, and we aren't responsible for them. SOLIDWORKS is a trademark of Dassault Systèmes, Google
          Drive is a trademark of Google LLC, and FIRST and FRC are trademarks of FIRST. Permafrost is not affiliated
          with or endorsed by any of them.
        </p>
      </Section>

      <Section id="source" title="Source code">
        <p>
          Permafrost's source code is published at <a href={REPO_URL}>{REPO_URL.replace("https://", "")}</a>. Any license
          file in that repository governs how you may use, copy or change the source code. These terms cover your use of
          the official releases and this site.
        </p>
      </Section>

      <Section id="warranty" title="No warranty">
        <p className="prose-caps">
          Permafrost and this site are provided "as is" and "as available", without warranties of any kind, whether
          express or implied, including warranties of merchantability, fitness for a particular purpose, non-infringement,
          and that the software will be error-free or uninterrupted. Some places don't allow these exclusions, so some of
          them may not apply to you.
        </p>
      </Section>

      <Section id="liability" title="Limitation of liability">
        <p className="prose-caps">
          To the fullest extent the law allows, we won't be liable for any indirect, incidental, special, consequential or
          punitive damages, or for any loss of data, files, work, profits or goodwill, arising from your use of, or
          inability to use, Permafrost or this site, even if we were told such damage was possible. Because Permafrost is
          free, our total liability for any claim will not be more than US $50, unless the law requires otherwise.
        </p>
      </Section>

      <Section id="ending" title="Stopping use">
        <p>
          You can stop using Permafrost at any time: sign out, revoke its access in your Google Account, and uninstall it.
          Sections of these terms that by their nature should continue (such as Your files, No warranty and Limitation of
          liability) keep applying after you stop.
        </p>
      </Section>

      <Section id="changes" title="Changes to these terms">
        <p>
          We may update these terms. When we do, we'll change the effective date at the top of this page and note it in
          the release notes on GitHub. If you keep using Permafrost after a change, you're agreeing to the updated terms.
        </p>
      </Section>

      <Section id="contact" title="Contact">
        <p>Questions about these terms?</p>
        <ContactLine />
      </Section>
    </LegalLayout>
  );
}
