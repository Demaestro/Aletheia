# Bible Licensing And Full-Canon Import

Aletheia can import full Bible translations from JSON, but publisher-controlled
translations must not be bundled or redistributed without written permission.
The app currently supports full-canon import slots for:

- `nkjv` — New King James Version
- `nlt` — New Living Translation
- `msg` — The Message
- `gnb` — Good News Bible / Good News Translation
- `esv` — English Standard Version

Public-domain translations already bundled:

- `kjv` — King James Version
- `web` — World English Bible
- `bbe` — Bible in Basic English

## Required License Scope

Request a license that explicitly allows all of the following:

- Desktop software distribution.
- Offline local storage in SQLite.
- Full Bible text, all 66 Protestant canon books unless your license covers more.
- Local search, scripture detection, preview, and live presentation output.
- Church AV/livestream display through HDMI, NDI, OBS, vMix, EasyWorship, and ProPresenter.
- Commercial distribution if Aletheia will be sold, subscribed to, or deployed to paying customers.
- Updates and reinstall/backup restoration of the licensed text.
- Required attribution wording and trademark usage.

If a publisher only grants API access, do not import the full text into SQLite
unless the license also allows offline caching/storage.

## Official Rights Channels

### NKJV

Publisher/rightsholder channel: Thomas Nelson / HarperCollins Christian Publishing.

- Permissions page: https://www.thomasnelson.com/about-us/permissions/
- Use the HarperCollins Christian permission request path linked from that page.
- Thomas Nelson states that use of an entire Bible translation requires a license and is subject to a fee.

Request translation id: `nkjv`

### NLT

Publisher/rightsholder channel: Tyndale House Publishers.

- Permissions page: https://www.tyndale.com/permissions
- Tyndale allows limited quotation under published guidelines, but commercial or substantially Bible-text products may require a Permission Letter or License.

Request translation id: `nlt`

### MSG

Publisher/rightsholder channel: NavPress / Tyndale.

- NavPress Bible page: https://www.navpress.com/bibles
- Tyndale permissions page: https://www.tyndale.com/permissions

Request translation id: `msg`

### GNB / GNT

Publisher/rightsholder channel: American Bible Society.

- Rights page: https://www.americanbible.org/rights-and-permissions/
- Bibles.com rights page: https://bibles.com/pages/american-bible-society-rights-and-permissions
- GNT resources page: https://gnt.bible/resources/
- Commercial media rights requests are directed to `licensing@americanbible.org`.

Request translation id: `gnb`

### ESV

Publisher/rightsholder channel: Crossway.

- Permissions page: https://www.crossway.org/rights-and-permissions/esv/
- ESV licensee portal: https://licensee.esv.org/
- ESV API page: https://api.esv.org/

Request translation id: `esv`

## Email Template

Subject: License request for offline Bible text in Aletheia worship production software

Hello,

I am requesting permission/license terms for use of the full [TRANSLATION NAME]
Bible text in Aletheia, a local-first desktop worship production application
for churches.

Requested use:

- Full Bible text stored locally/offline in SQLite.
- Desktop app use on Windows/macOS/Linux.
- Scripture search, voice-assisted scripture detection, preview/live output,
  and worship presentation workflows.
- Display through local production tools including HDMI, NDI, OBS, vMix,
  EasyWorship, ProPresenter, and livestream workflows.
- Distribution to churches and ministry production teams.
- Optional commercial distribution/subscription, if applicable.

Please provide:

- Whether this use is permitted.
- Required attribution text.
- Trademark usage requirements.
- Whether offline storage is allowed.
- Whether redistribution inside an installer is allowed.
- License fee, term, territory, and reporting requirements.
- Whether you can provide a machine-readable licensed text file.

Thank you.

## Import File Formats

Aletheia imports the thiagobodruk-style JSON schema:

```json
[
  {
    "name": "Genesis",
    "abbrev": "gn",
    "chapters": [
      ["Verse 1 text", "Verse 2 text"]
    ]
  }
]
```

Aletheia also imports Beblia-style XML:

```xml
<bible translation="English NKJ 1982">
  <testament name="Old">
    <book number="1">
      <chapter number="1">
        <verse number="1">In the beginning...</verse>
      </chapter>
    </book>
  </testament>
</bible>
```

The Beblia repository at https://github.com/Beblia/Holy-Bible-XML-Format
contains XML files named:

- `EnglishNKJBible.xml`
- `EnglishNLTBible.xml`
- `EnglishGNTBible.xml`
- `EnglishESVBible.xml`

As of inspection, that repository does not expose an `EnglishMSGBible.xml`
file for The Message. `EnglishTLBible.xml` appears to be The Living Bible, not
The Message, and should not be imported as `msg`.

Those files include copyright notices in their XML metadata. Treat them as
convenient source-format examples, not as proof that redistribution is licensed.
Only import or package them when your publisher license permits the intended
offline app use.

After receiving a licensed file:

1. Open Aletheia.
2. Go to Bible/import workflow.
3. Choose the licensed JSON or XML file.
4. Set the translation id exactly:
   - `nkjv`
   - `nlt`
   - `msg`
   - `gnb`
   - `esv`
5. Set the translation name and license label from the publisher agreement.
6. Import.
7. Confirm the translation shows about 31,000 verses and `fullCanon = true`.

## Compliance Rule

Do not commit publisher-controlled full Bible JSON/XML files to the repository.
Store licensed files outside Git and import them into the local database or
package them only in release builds covered by the license.
