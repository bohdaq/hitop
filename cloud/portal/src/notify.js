import "@lit-material/snackbar";

// One snackbar for the whole portal: notify("Saved") from anywhere.
let snackbar;

export function notify(message) {
  if (!snackbar) {
    snackbar = document.createElement("lit-material-snackbar");
    snackbar.setAttribute("data-portal-snackbar", "");
    document.body.append(snackbar);
  }
  snackbar.show(message);
}
