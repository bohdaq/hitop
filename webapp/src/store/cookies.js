// The store's cookies (a part of Store, src/store.js).


export const CookiesPart = (Base) =>
  class extends Base {
  setCookieJarEnabled(on) {
    this.cookieJarEnabled = on;
    this.emit();
  }

  setCookies(cookies) {
    this.cookies = cookies;
    this.emit();
  }
  };
