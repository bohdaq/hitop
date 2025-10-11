# Firefox Add-ons Submission

## Extension Name
HITOP - HTTP API Testing Tool

## Summary (250 characters max)
A powerful HTTP API testing tool with collections, variables, and custom scripting. Test APIs, manage requests, automate workflows, and organize your work with collections - all in your browser.

## Description (Full)

HITOP is a comprehensive HTTP API testing tool that brings professional API testing capabilities directly to your Firefox browser. Whether you're a developer testing your own APIs or a QA engineer validating endpoints, HITOP provides all the tools you need.

**Core Features:**

🚀 **Complete HTTP Support**
• All HTTP methods: GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS
• Custom headers and request bodies
• Syntax-highlighted responses (JSON, XML, HTML, text)
• Clear status codes with descriptions
• Full response header visibility

📁 **Collections & Organization**
• Group related requests into collections
• Save and reuse requests
• Run entire collections sequentially
• Drag & drop to reorder requests
• Import/Export collections as JSON
• Share collections with your team

🔧 **Variables & Dynamic Requests**
• Define collection-level variables
• Use ${variableName} syntax in URLs, headers, and bodies
• Reuse common values across requests
• Easy variable management interface

⚡ **Custom Scripting**
• Pre-request scripts to modify requests before sending
• Post-request scripts to extract and process response data
• Share data between requests using context
• Automate complex workflows
• JavaScript-based scripting engine

📊 **Request History**
• Track all requests and collection runs
• Reload previous requests
• Filter by success/failure
• Review past API calls

🎯 **Multiple Tabs**
• Work on multiple requests simultaneously
• Switch between tabs easily
• Each tab maintains its own state

**Privacy & Security:**
• All data stored locally in your browser
• No external servers or data collection
• No tracking or analytics
• Open source and transparent
• You control your data

**Perfect For:**
• API developers testing endpoints
• QA engineers validating APIs
• DevOps teams monitoring services
• Anyone working with REST APIs
• Teams needing to share API collections

**Why Choose HITOP?**
• Clean, intuitive interface
• Powerful automation with scripts
• Complete privacy - no cloud required
• Free and open source (MIT License)
• Active development and support

**Getting Started:**
1. Click the HITOP icon in your toolbar
2. Enter a URL and select HTTP method
3. Click "Make Request"
4. View the response with syntax highlighting

Start testing your APIs efficiently with HITOP today!

## Categories
- Developer Tools
- Productivity

## Tags (20 max)
- api
- rest
- http
- testing
- developer
- postman
- api-testing
- rest-client
- http-client
- developer-tools
- api-client
- collections
- variables
- scripting
- automation
- json
- requests
- debugging
- web-development
- productivity

## Support Email
(Your email address)

## Support Website
https://github.com/bohdaq/hitop

## Homepage
https://github.com/bohdaq/hitop

## Privacy Policy URL
https://github.com/bohdaq/hitop/blob/main/PRIVACY.md

## License
MIT License

## Version Notes (for 1.0.0)

**Initial Release - Version 1.0.0**

HITOP brings professional API testing to Firefox with:

✨ **New Features:**
• Complete HTTP method support (GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS)
• Request collections for organization
• Collection variables with ${} interpolation
• Pre-request and post-request custom scripts
• Request history tracking
• Multiple tabs support
• Import/Export collections
• Drag & drop request reordering
• Syntax-highlighted responses
• Full header management

🔒 **Privacy:**
• All data stored locally
• No external servers
• No tracking or analytics
• Open source code

📝 **Documentation:**
• Comprehensive scripting guide
• Variables guide
• Extension guide
• Privacy policy

This is the first stable release of HITOP for Firefox. We're excited to bring powerful API testing tools to your browser!

## Screenshots Descriptions

**Screenshot 1: Main Interface**
HITOP's clean interface showing request configuration with URL, method, headers, and body fields. Response viewer displays syntax-highlighted JSON.

**Screenshot 2: Collections Sidebar**
Organize your API requests into collections. Drag and drop to reorder, run entire collections, and manage variables.

**Screenshot 3: Variables Management**
Define collection-level variables and use them across requests with ${variableName} syntax for dynamic values.

**Screenshot 4: Custom Scripts**
Write pre-request and post-request scripts to automate workflows, extract data, and share context between requests.

**Screenshot 5: Request History**
Track all requests and collection runs. Filter by success/failure and reload previous requests with one click.

**Screenshot 6: Multiple Tabs**
Work on multiple API requests simultaneously with tab support. Each tab maintains its own state.

## Promotional Text (Short - for listings)

Test APIs like a pro! HITOP brings powerful API testing to Firefox with collections, variables, custom scripts, and complete privacy. All data stays local - no cloud required. Free & open source.

## Developer Comments (for reviewers)

**Technical Details:**

This extension packages a React application that provides API testing functionality. Key technical aspects:

1. **Permissions Justification:**
   - `<all_urls>`: Required to make HTTP requests to any API endpoint for testing purposes
   - Data is saved using localStorage (no storage permission required)
   - Note: `tabs.create()` doesn't require tabs permission (only needed for reading tab data)

2. **Content Security Policy:**
   - Uses `'unsafe-eval'` to enable custom JavaScript scripting feature
   - This is essential for pre/post-request scripts functionality
   - Scripts run in a sandboxed context with limited API access

3. **Privacy:**
   - No external network requests except user-initiated API tests
   - All data stored in localStorage
   - No analytics, tracking, or telemetry
   - Open source: https://github.com/bohdaq/hitop

4. **Build Process:**
   - React app built with Create React App
   - Uses relative paths (homepage: ".") for extension compatibility
   - Source code available for review

5. **Testing:**
   - Tested on Firefox 58.0+
   - All features functional
   - No known issues

The extension is designed to be a complete API testing solution while maintaining user privacy and security.

## Additional Information

**Source Code Repository:**
https://github.com/bohdaq/hitop

**Documentation:**
- Extension Guide: https://github.com/bohdaq/hitop/blob/main/EXTENSION_GUIDE.md
- Scripting Guide: https://github.com/bohdaq/hitop/blob/main/SCRIPTING_GUIDE.md
- Variables Guide: https://github.com/bohdaq/hitop/blob/main/VARIABLES_GUIDE.md
- Privacy Policy: https://github.com/bohdaq/hitop/blob/main/PRIVACY.md

**License:**
MIT License - https://github.com/bohdaq/hitop/blob/main/LICENSE

**Support:**
For issues, questions, or feature requests, please open an issue on GitHub:
https://github.com/bohdaq/hitop/issues

**Roadmap:**
- Chrome extension version
- Export to various formats (cURL, Postman, etc.)
- Environment management
- Request chaining improvements
- Team collaboration features

We welcome contributions and feedback from the community!
