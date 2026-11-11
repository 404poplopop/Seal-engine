# Seal Engine License (SEL)

SPDX identifier: `LicenseRef-Seal-Proprietary`

The Seal source code is publicly viewable, but Seal is **not open source**. Reading the code does not give You any right beyond this license.

Copyright (c) 2026 404poplopop. All rights reserved except as granted below.

## 1. Definitions

- **"Seal"** means the Seal Engine source code, binaries, editor, tools and documentation in this repository, and every update the Licensor publishes. It does not include Seal Content.
- **"Seal Content"** means example projects, templates, sample art, audio and other assets the Licensor ships with Seal.
- **"Licensor"** means 404poplopop.
- **"You"** means the person or organization using Seal.
- **"Licensee"** means anyone who has accepted this license.
- **"Product"** means a game, application or other work You build with Seal and distribute or sell.
- **"Mod"** means content or code that extends or changes a Product through the Product's own mod support.
- **"Gross Revenue"** has the meaning in Section 3.
- **"Paid Tools"** means any tool the Licensor sells separately, including the migration tools for moving projects from other engines. Paid Tools are not covered by this license.

## 2. Grant

Subject to this license, the Licensor grants You a worldwide, non-exclusive, non-transferable, non-sublicensable license to:

1. Use Seal to make Products, for any purpose, including commercial.
2. Read and modify the Seal source code for Your own Products and internal tooling.
3. Distribute the Seal runtime, as compiled or compressed code that is built into Your Product, as part of that Product.
4. Obfuscate, encrypt or otherwise protect Your Product's code and data, including the Seal runtime inside it.
5. Use and modify Seal Content inside Your Products (Section 6).

## 3. Royalty

1. **Rate and threshold.** A Product owes **4%** of its Gross Revenue above **US$250,000 lifetime per Product**.
2. **Gross Revenue** is all money You or Your affiliates receive from the Product worldwide, before deducting platform fees, taxes, refunds, publisher cuts or costs. It includes:
   - sales, subscriptions, in-app purchases and DLC;
   - advertising, sponsorship and licensing income from the Product;
   - money from paid user-generated content or paid Mods sold through the Product.
   It does not include: crowdfunding raised before release (until the Product is sold), grants, work-for-hire fees paid for building a Product for someone else, or money from free or non-commercial use.
3. **No royalty** is owed on the first US$250,000, on free Products, on non-commercial or educational use, or on Mods that are given away free.
4. **Worked example.** A Product earns US$300,000 gross lifetime. Royalty is 4% of US$50,000, which is US$2,000.
5. **Notify before ship.** Before releasing a Product commercially, You tell the Licensor its name and platform by email to seal@poplocorp.com. This is a notice only. It does not need approval.
6. **Quarterly report or skip.** After the Product crosses the threshold, You send a short report of that quarter's Gross Revenue within 30 days after the quarter ends. If a quarter has no royalty-bearing revenue, You may mark it "skip".
7. **Good faith.** Reports are made in good faith and the Licensor may ask for reasonable proof of the figures once per calendar year.
8. **Audit right.** The Licensor may audit Your records of a Product's Gross Revenue, directly or through an independent auditor bound by confidentiality, to check the royalty reports.
   - The Licensor gives You at least 30 days' written notice and audits during normal business hours, at most once per calendar year, unless an earlier audit found a shortfall.
   - You give access to the books, sales and platform reports and other records that show the Product's Gross Revenue, for the current period and the previous 3 years.
   - You pay any shortfall the audit finds, plus interest under item 9.
   - The Licensor pays the cost of the audit. If the audit finds that You underpaid by more than 5% of the royalty owed for the audited period, You pay the reasonable cost of the audit as well.
9. **Payment.** Royalties are paid in US dollars, due with each report. Late amounts carry interest of 1% per month, or the highest rate the law allows if that is lower.

## 4. Modding

1. Mods are encouraged. If a Product ships with mod support, You may use the Seal modding APIs and tools to give players full mod support.
2. Mod authors keep their rights in their Mods. Free Mods do not owe a royalty by themselves.
3. If You choose to obfuscate or protect a Product's code, You may remove or restrict its modding tools entirely. Seal does not require modding to be left on.

## 5. Sharing Seal source code

1. Do not publish, sell, sublicense or redistribute the Seal source code or editor, in whole or in part, in source form, except:
   - to another Licensee for work on the same Seal version;
   - patches or contributions sent to the official Seal repository;
   - public discussion snippets of up to 30 lines, with a link to Seal;
   - the compiled runtime as part of a Product (Section 2.3);
   - the Licensor's own public repository, where the source is viewable under this license.
2. Do not ship the Seal editor to the general public with a Product, unless the Product's mod support needs it. If You do, the editor is provided under its own terms, and any fork or modified copy of it follows this license.
3. Do not publish a modified copy of Seal as a competing engine, toolkit or editor.
4. Do not remove copyright, license or attribution notices.
5. Do not use Paid Tools without a separate license from the Licensor.
6. If You pass Seal source to another Licensee, You stay responsible for keeping it from leaking.

## 6. Seal Content

1. Seal Content may be used and modified inside Products. It may not be sold or redistributed on its own, or in a pack that competes with Seal Content.
2. Third-party assets inside Seal Content keep their own licenses, listed in `THIRD_PARTY_NOTICES.md`.

## 7. Credit and trademarks

1. If a Product has a credits screen, it shows "Made with Seal Engine". If it has no credits, a mention on the store page or website is enough.
2. "Seal", "Seal Engine" and the Seal logo are trademarks of the Licensor. Using the logo beyond the credit line needs the Licensor's written permission first.
3. Do not use the Seal name or logo to suggest the Licensor endorses You or Your Product.

## 8. Third-party components

1. Seal bundles third-party components that keep their own licenses. They are listed in `THIRD_PARTY_NOTICES.md`.
2. Seal does not include copyleft-licensed dependencies. A copyleft dependency is a bug and will be removed.
3. Pasting copyleft or incompatible third-party code into Seal or a contribution is not allowed. Ideas and techniques are free; copied code is not.

## 9. Contributions and clean room

1. By sending a contribution, You grant the Licensor a perpetual, worldwide, irrevocable, royalty-free license to use, modify, sublicense and distribute it as part of Seal, and You confirm You have the right to do so.
2. You confirm Your contribution was written by You, or generated under Seal's agent rules, and does not copy code from any other engine, or any project whose license does not allow it. Seal is developed as a clean room from the start.
3. The Licensor may reject, revert or rewrite any contribution that looks copied or too similar to existing code.

## 10. AI training

You may not use the Seal source code to train or fine-tune a machine-learning model that is distributed or offered to others, unless the Licensor gives written permission. Using AI assistants on Your own Products is allowed.

## 11. Player-facing terms

You are responsible for the terms between You and Your players. Your Product's end-user license should say that the Seal runtime in it is provided "as is" and that the Licensor is not liable to players.

## 12. Updates and versions

1. A Product keeps the license terms of the Seal version it was built with, if later versions change the terms.
2. The Licensor may change this license for future versions. A change never raises the royalty rate for a Product that already shipped.

## 13. No warranty and limit of liability

SEAL IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NON-INFRINGEMENT. THE LICENSOR IS NOT LIABLE FOR ANY INDIRECT, INCIDENTAL OR CONSEQUENTIAL DAMAGES, LOST PROFITS OR LOST DATA, AND TOTAL LIABILITY IS LIMITED TO THE ROYALTIES YOU PAID IN THE 12 MONTHS BEFORE THE CLAIM.

## 14. Termination

1. The Licensor may end Your license for a Product if You break Section 5 or do not pay a royalty owed, and do not fix it within 30 days of written notice.
2. Unpaid royalties stay owed after termination.
3. Products already released keep the right to distribute the compiled runtime they contain, as long as royalties are paid.
4. After termination of Your license, You must stop using Seal source for new work and delete copies You received from the Licensor, other than the runtime inside released Products.

## 15. Public source and takedowns

The Seal source code is public, but it is not open source. It is made viewable under this license, and copying, redistributing or using it outside this license is not allowed. If Seal source code is copied or published against this license, the Licensor may send takedown notices to the hosts. Report suspected leaks or infringement to seal@poplocorp.com.

## 16. Whole agreement

This license is the whole agreement about Seal between You and the Licensor, unless You have a separate written agreement signed by both.
