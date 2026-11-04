import { ContactLine, LegalLayout, Section, type TocItem } from "./LegalLayout";
import { REPO_URL } from "../lib/release";

const TOC: TocItem[] = [
  { id: "who", label: "Who we are" },
  { id: "google-data", label: "Google data Permafrost accesses" },
  { id: "use", label: "How the data is used" },
  { id: "limited-use", label: "Google API Services User Data Policy" },
  { id: "storage", label: "Where data is stored" },
  { id: "sharing", label: "Who can see it" },
  { id: "security", label: "Security" },
  { id: "retention", label: "Keeping and deleting data" },
  { id: "website", label: "This website" },
  { id: "children", label: "Students and children" },
  { id: "changes", label: "Changes to this policy" },
  { id: "contact", label: "Contact" },
];

const GOOGLE_POLICY = "https://developers.google.com/terms/api-services-user-data-policy";
const GOOGLE_CONNECTIONS = "https://myaccount.google.com/connections";

export function PrivacyPolicy() {
  return (
    <LegalLayout
      eyebrow="Legal"
      title="Privacy Policy"
      toc={TOC}
      summary={
        <>
          <p className="legal-summary-title">The short version</p>
          <ul>
            <li>Permafrost is a desktop app. It runs on your computer and talks directly to Google Drive.</li>
            <li>
              We don't run a server that receives your Google data, so we never see your files, your account details or
              your sign-in.
            </li>
            <li>Your Google data is used only to sync your team's CAD files. It is never sold or used for ads.</li>
            <li>You can sign out in the app and revoke access from your Google Account at any time.</li>
          </ul>
        </>
      }
    >
      <p>
        This policy explains what information the Permafrost desktop app ("Permafrost", "the app") and the Permafrost
        website ("the site") handle, why, and the choices you have. It applies to the official Permafrost releases and to
        this site.
      </p>

      <Section id="who" title="Who we are">
        <p>
          Permafrost is a free app for sharing SolidWorks files through Google Drive, built for FIRST Robotics Competition
          teams. It is maintained by the Permafrost project ("we", "us"), and its source code is public at{" "}
          <a href={REPO_URL}>{REPO_URL.replace("https://", "")}</a>, so you can check everything this policy describes.
        </p>
      </Section>

      <Section id="google-data" title="Google data Permafrost accesses">
        <p>When you choose Sign in with Google, Permafrost asks for these permissions:</p>
        <dl className="prose-dl">
          <dt>
            See, edit, create and delete your Google Drive files <code>(…/auth/drive)</code>
          </dt>
          <dd>
            Your team's CAD lives in an existing shared drive or folder, usually created by teammates or other tools, so
            Permafrost needs to work with files it didn't create. With this permission, the app:
            <ul>
              <li>lists the shared drives and folders you can access, so you can pick your team's CAD folder;</li>
              <li>
                reads the names, folder structure and details of files in that folder (IDs, sizes, checksums,
                modification times, who last changed each file, and Permafrost's own lock information), and downloads the
                files you choose to keep on your computer;
              </li>
              <li>uploads new files and new versions of files when you push your changes;</li>
              <li>
                marks files as checked out and releases them afterwards, by setting Drive's read-only content restriction
                and adding hidden app properties (your email address, your name, your computer's name and the time);
              </li>
              <li>
                writes a small record of every push to a <code>.permafrost</code> folder inside your CAD
                folder (your email address, your name, your message, the time and which file versions went up);
              </li>
              <li>moves a file to the shared drive's trash when you choose Delete.</li>
            </ul>
            Apart from listing drives and folders while you set it up, Permafrost only works inside the CAD folder you
            choose.
          </dd>
          <dt>
            Your email address and basic sign-in identity <code>(openid, email)</code>
          </dt>
          <dd>
            Used to show which account you're signed in with, and to label your check-outs and pushes so teammates can
            see who has a file.
          </dd>
        </dl>
      </Section>

      <Section id="use" title="How the data is used">
        <p>
          Permafrost uses Google data only to provide the features you see in the app: browsing, downloading, checking
          out, pushing changes and showing history for your team's CAD files. Specifically, we don't:
        </p>
        <ul>
          <li>sell Google data or any other user data;</li>
          <li>use it for advertising, including retargeting or personalized ads;</li>
          <li>use it to work out credit-worthiness or for lending;</li>
          <li>use it to train artificial intelligence or machine learning models;</li>
          <li>
            let any person read it. We have no way to: it stays on your computer and in your Google Drive and is never
            sent to us.
          </li>
        </ul>
        <p>Permafrost has no analytics, telemetry, ads or tracking of any kind.</p>
      </Section>

      <Section id="limited-use" title="Google API Services User Data Policy">
        <p>
          Permafrost's use and transfer to any other app of information received from Google APIs will adhere to the{" "}
          <a href={GOOGLE_POLICY}>Google API Services User Data Policy</a>, including the Limited Use requirements.
        </p>
      </Section>

      <Section id="storage" title="Where data is stored">
        <h3>On your computer</h3>
        <ul>
          <li>
            <strong>Your sign-in.</strong> A Google refresh token, kept in your operating system's credential store
            (Windows Credential Manager or the macOS Keychain). Short-lived access tokens are kept only in memory.
          </li>
          <li>
            <strong>A local index.</strong> A database of file and folder details from your CAD folder, so browsing
            doesn't wait on the network. It's kept in the app's data folder.
          </li>
          <li>
            <strong>Your CAD files.</strong> The files you choose to sync, in the vault folder you pick.
          </li>
          <li>
            <strong>Settings</strong>, such as which folder you chose.
          </li>
          <li>
            <strong>Logs and crash reports.</strong> These are written to help with troubleshooting and may include file
            names and email addresses. They stay on your computer and are never uploaded. Logs are deleted after 14
            days. If you share a log when reporting a problem, you decide what to send.
          </li>
        </ul>
        <p>
          On Windows, Permafrost can also ask SolidWorks, running on the same computer, which files an assembly uses.
          This happens entirely on your computer.
        </p>
        <h3>In your Google Drive</h3>
        <p>
          The files you push, the lock information and the push records described above are stored in your
          team's Google Drive, under your team's Drive settings and{" "}
          <a href="https://policies.google.com/privacy">Google's Privacy Policy</a>.
        </p>
      </Section>

      <Section id="sharing" title="Who can see it">
        <p>
          We don't receive your data, so we don't share it with anyone. Data moves only between your computer and Google.
        </p>
        <p>
          The people who already have access to your team's shared drive or folder can see what Permafrost writes there,
          including who has a file checked out, from which computer, and the history of pushes. That access is
          controlled by your team's Drive sharing settings, not by Permafrost.
        </p>
      </Section>

      <Section id="security" title="Security">
        <ul>
          <li>All traffic between the app and Google is encrypted with HTTPS.</li>
          <li>
            Sign-in uses Google's OAuth flow for desktop apps with PKCE, through your web browser. Permafrost never sees
            your Google password.
          </li>
          <li>Your refresh token is kept in your operating system's secure credential store, not in a plain file.</li>
          <li>Files you haven't checked out are made read-only on disk, so they can't be overwritten by accident.</li>
        </ul>
        <p>No system is perfectly secure, so please also keep your computer and Google account protected.</p>
      </Section>

      <Section id="retention" title="Keeping and deleting data">
        <ul>
          <li>
            <strong>Sign out</strong> in Permafrost's Settings to delete the saved sign-in from your computer.
          </li>
          <li>
            <strong>Revoke access</strong> at any time from your Google Account at{" "}
            <a href={GOOGLE_CONNECTIONS}>myaccount.google.com/connections</a>. Permafrost then can't reach your Drive
            until you sign in again.
          </li>
          <li>
            <strong>Remove local data</strong> by uninstalling the app and deleting its data folder (Windows:{" "}
            <code>%APPDATA%\org.frc.cadsync</code> and <code>%LOCALAPPDATA%\org.frc.cadsync</code>; macOS:{" "}
            <code>~/Library/Application Support/org.frc.cadsync</code> and <code>~/Library/Logs/org.frc.cadsync</code>),
            plus your vault folder if you no longer want the synced files.
          </li>
          <li>
            <strong>Data in Google Drive</strong> stays there until you or your drive's managers delete it, like any
            other Drive content. That includes the <code>.permafrost</code> folder.
          </li>
        </ul>
        <p>Because we don't hold any of your data, there's nothing for us to keep or delete on our side.</p>
      </Section>

      <Section id="website" title="This website">
        <ul>
          <li>The site has no accounts, cookies, analytics or ads, and its fonts are hosted on the site itself.</li>
          <li>
            It is hosted on Vercel, which may record standard request details such as your IP address and browser type
            to run and protect the service (see{" "}
            <a href="https://vercel.com/legal/privacy-policy">Vercel's privacy policy</a>).
          </li>
          <li>
            Download buttons look up the latest release on GitHub, and the installers are downloaded from GitHub (see{" "}
            <a href="https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement">
              GitHub's privacy statement
            </a>
            ).
          </li>
        </ul>
      </Section>

      <Section id="children" title="Students and children">
        <p>
          Permafrost is made for high school robotics teams. It isn't directed at children under 13, and we don't knowingly
          handle personal information from them. If you sign in with a school or organization Google account, your
          administrator may control which apps can access it, and their policies apply too.
        </p>
      </Section>

      <Section id="changes" title="Changes to this policy">
        <p>
          If what Permafrost does with data changes, we'll update this page and the effective date at the top, and note
          the change in the release notes on GitHub before it takes effect.
        </p>
      </Section>

      <Section id="contact" title="Contact">
        <p>Questions about privacy, or a request about your data?</p>
        <ContactLine />
      </Section>
    </LegalLayout>
  );
}
