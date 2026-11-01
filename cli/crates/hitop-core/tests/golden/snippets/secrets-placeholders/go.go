package main

import (
	"fmt"
	"io"
	"net/http"
	"strings"
)

func main() {
	body := strings.NewReader("plain text\twith a tab")
	req, err := http.NewRequest("PATCH", "https://api.example.test/tenants/{{tenant}}/note", body)
	if err != nil {
		panic(err)
	}
	req.Header.Add("X-Trace", "t-1")
	req.Header.Add("Authorization", "Bearer {{token}}")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()
	out, _ := io.ReadAll(resp.Body)
	fmt.Println(resp.Status)
	fmt.Println(string(out))
}
