# The 30 second terminal GIF: import a Postman collection, run it, see it fail the way CI would.
# Record from the repository root with vhs (https://github.com/charmbracelet/vhs) and hitop 0.3.0
# on PATH:
#   vhs site/demo/demo.tape          # writes site/demo/demo.gif
# Requests go to the launch-gate mock servers on 127.0.0.1 (cli/fixtures/launch-gate), started
# and stopped in hidden steps; nothing leaves the machine.

Output site/demo/demo.gif

Require hitop
Require python3

Set Shell "bash"
Set FontSize 18
Set Width 1200
Set Height 720
Set Padding 24
Set TypingSpeed 45ms
Set Theme "Catppuccin Mocha"

Hide
Type "export HITOP_KEYCHAIN=off DEMO=$(mktemp -d) && cp cli/fixtures/launch-gate/*.json $DEMO/ && cd $DEMO" Enter
Type "python3 $OLDPWD/cli/fixtures/launch-gate/mock_server.py 8781 staging demo-pw >/dev/null 2>&1 & MOCK=$!" Enter
Type "export PS1='$ ' && clear" Enter
Show

Type "# A Postman collection and its environments, into a folder for git" Enter
Sleep 600ms
Type "hitop import shop.postman_collection.json staging.postman_environment.json local.postman_environment.json --to shop-api" Enter
Sleep 2s
Type "ls shop-api/requests/*" Enter
Sleep 2s

Type "clear" Enter
Type "# Log in, pass the token on, check the orders" Enter
Sleep 600ms
Type "hitop run shop-api --env Staging --secret password=demo-pw" Enter
Sleep 3s

Type "clear" Enter
Type "# A wrong password fails the run, with exit code 1 for CI" Enter
Sleep 600ms
Type "hitop run shop-api --env Staging --secret password=wrong --reporter cli,junit --out reports; echo exit $?" Enter
Sleep 4s
Type "grep -o 'testcase name=\"[^\"]*\"[^>]*file=\"[^\"]*\"' reports/junit.xml" Enter
Sleep 4s

Hide
Type "kill $MOCK; cd /; rm -rf $DEMO" Enter
