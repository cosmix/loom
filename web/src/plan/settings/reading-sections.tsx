import { useAtom, useAtomValue } from "jotai";
import { useId, type ReactElement } from "react";

import {
  DEFAULT_READING_FACE,
  READING_FACES,
  READING_SIZES,
  readingFaceAtom,
  readingSizeAtom,
  sourceMarksAtom,
} from "./preferences";
import { PaneBlock, SettingsPane } from "./settings-pane";
import { ToggleRow } from "./settings-sections";
import { SizePicture } from "./size-picture";
import "./reading.css";

const PREVIEW_ENGLISH =
  "The app stores no workflow state and works out the current step from the earliest gap in the doc/ tree.";
const PREVIEW_GREEK = "Η εφαρμογή δεν αποθηκεύει κατάσταση ροής εργασίας.";

/**
 * The specimen: the two preview lines in the chosen face and size, framed as a figure. It sits
 * above the face and size choices, so it reflects both controls under it. The eyebrow is hidden
 * from assistive technology because the figure's label already names it.
 */
function ReadingPreview(): ReactElement {
  const face = useAtomValue(readingFaceAtom);
  const size = useAtomValue(readingSizeAtom);
  const family = READING_FACES.find((option) => option.id === face)!.family;
  return (
    <figure aria-label="Reading preview" className="ps-preview">
      <figcaption className="ps-preview-eyebrow" aria-hidden="true">
        Preview
      </figcaption>
      <div style={{ fontFamily: family, fontSize: size, lineHeight: 1.6 }}>
        <p lang="en">{PREVIEW_ENGLISH}</p>
        <p lang="el">{PREVIEW_GREEK}</p>
      </div>
    </figure>
  );
}

function ReadingFaceBlock(): ReactElement {
  const [selected, setSelected] = useAtom(readingFaceAtom);
  const name = useId();
  return (
    <PaneBlock title="Face" hint="Document text only; titles and controls stay Alegreya Sans">
      <div role="radiogroup" aria-label="Reading face" className="ps-faces">
        {READING_FACES.map((face) => (
          <label key={face.id} className="ps-face">
            <input
              type="radio"
              name={name}
              aria-label={face.label}
              checked={selected === face.id}
              onChange={() => setSelected(face.id)}
              className="sr-only"
            />
            {face.id === DEFAULT_READING_FACE && <span className="ps-flag">Default</span>}
            <span className="ps-specimen" style={{ fontFamily: face.family }} aria-hidden="true">
              <span className="ps-specimen-latin">Aa</span>
              <span className="ps-specimen-greek" lang="el">
                Ωω
              </span>
            </span>
            <span className="ps-face-name">{face.label}</span>
            <span className="ps-face-caption">{face.scripts}</span>
          </label>
        ))}
      </div>
    </PaneBlock>
  );
}

/** Three tiles in a row, each a page picture over its label; the native radio group keeps the keys. */
function ReadingSizeBlock(): ReactElement {
  const [selected, setSelected] = useAtom(readingSizeAtom);
  const name = useId();
  return (
    <PaneBlock title="Size">
      <div role="radiogroup" aria-label="Reading size" className="ps-seg">
        {READING_SIZES.map(({ size, label }) => (
          <label key={size} className="ps-seg-item">
            <input
              type="radio"
              name={name}
              aria-label={`${label} ${size}px`}
              checked={selected === size}
              onChange={() => setSelected(size)}
              className="sr-only"
            />
            <SizePicture size={size} />
            <span className="ps-seg-label">{label}</span>
            <span className="ps-seg-px">{size}px</span>
          </label>
        ))}
      </div>
    </PaneBlock>
  );
}

/** The Reading pane: the specimen over the face and size choices, then the source marks switch. */
export function ReadingPane(): ReactElement {
  const [sourceMarks, setSourceMarks] = useAtom(sourceMarksAtom);
  return (
    <SettingsPane title="Reading" scope="browser">
      <div className="ps-block">
        <ReadingPreview />
      </div>
      <ReadingFaceBlock />
      <ReadingSizeBlock />
      <div className="ps-block ps-rows">
        <ToggleRow
          title="Show source marks after sentences"
          subtitle="The coloured squares that say which document a sentence draws on"
          checked={sourceMarks}
          onChange={() => setSourceMarks(!sourceMarks)}
        />
      </div>
    </SettingsPane>
  );
}
