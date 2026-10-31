// Phosphor icons (skill B6) as inline SVG strings, bundled by Vite.
import { unsafeSVG } from "lit/directives/unsafe-svg.js";

// Only the icons the app uses: a glob of the whole set would bundle ~1,500.
const files = import.meta.glob(
  "../node_modules/@phosphor-icons/core/assets/regular/{plus,x,floppy-disk,paper-plane-tilt,folder-simple,folders,sliders-horizontal,file-arrow-down,file-arrow-up,clock-counter-clockwise,play,dots-three-vertical,list,sun,moon,trash,copy,pencil-simple,arrow-left,house,dots-six-vertical,check-circle,warning-circle,plugs,globe}.svg",
  {
    query: "?raw",
    import: "default",
    eager: true,
  },
);

const byName = Object.fromEntries(
  Object.entries(files).map(([path, svg]) => [
    path.split("/").pop().replace(".svg", ""),
    svg.replace("<svg ", '<svg aria-hidden="true" focusable="false" '),
  ]),
);

export const icon = (name) => unsafeSVG(byName[name] ?? "");
