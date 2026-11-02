// Phosphor icons (skill B6), registered with lit-material's icon registry.
import { registerIcons } from "@lit-material/icon";

// Only the icons the app uses: a glob of the whole set would bundle ~1,500.
const files = import.meta.glob(
  "../node_modules/@phosphor-icons/core/assets/regular/{plus,x,floppy-disk,paper-plane-tilt,folder-simple,folders,sliders-horizontal,file-arrow-down,file-arrow-up,clock-counter-clockwise,play,dots-three-vertical,list,sun,moon,trash,copy,pencil-simple,arrow-left,house,dots-six-vertical,check-circle,warning-circle,plugs,globe,cookie,folder-open,file-zip,arrows-clockwise,code,git-diff,bookmark-simple,plugs-connected,arrow-up,arrow-down,info,caret-down,caret-right,keyboard,magnifying-glass,warning,text-align-left,eye,brackets-curly,gear-six,caret-up,columns,rows,copy-simple,square-split-horizontal}.svg",
  {
    query: "?raw",
    import: "default",
    eager: true,
  },
);

registerIcons(Object.fromEntries(Object.entries(files).map(([path, svg]) => [path.split("/").pop().replace(".svg", ""), svg])));

export { icon } from "@lit-material/icon";
