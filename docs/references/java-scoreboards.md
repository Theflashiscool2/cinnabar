# Native Java sidebar

The Java session decodes objective, display-slot, score and reset-score packets into
native retained state. It does not send synthetic Bedrock events. Team membership,
colour slots and prefixes/suffixes use the existing Java roster. Respawn retains
objectives; repeated login, reconfiguration and disconnect retire them.

`tools/java-reference/ProbeSidebar.java` calls the pinned Minecraft 26.3 packet
stream codecs to produce independent wire vectors, including blank, styled and
fixed number formats. Decoder tests check complete consumption and truncation at
every byte. The native state selects at most fifteen visible entries, descending
score and case-insensitive holder order, excluding holders beginning with `#`.

The application publishes changes to the existing JSON-UI sidebar and shared font
atlas. Unchanged score/team revisions do not rebuild formatted rows. Per-score
number formats override the objective's default; an absent default uses red
integer text. Literal components and the existing named-colour/bold/italic text
path are supported. Translation, arbitrary RGB, custom font, underline and
strikethrough components remain explicit presentation gaps; attempt reports record
`java_sidebar_component_not_implemented` rather than claiming those were rendered.
Player-list and below-name score surfaces remain separate unfinished capabilities.

A structural or codec pass is not rendered, iPhone or Hypixel acceptance. Those
require actual application output and server state. The iPhone acceptance table
tracks the current evidence separately.
