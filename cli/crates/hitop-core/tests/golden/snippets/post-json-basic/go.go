package main

import (
	"fmt"
	"io"
	"net/http"
	"strings"
)

func main() {
	body := strings.NewReader("{\n  \"name\": \"it's \\\"quoted\\\"\",\n  \"path\": \"C:\\\\tmp\",\n  \"shell\": \"$HOME `id`\",\n  \"city\": \"Kraków\"\n}")
	req, err := http.NewRequest("POST", "https://api.example.test/items", body)
	if err != nil {
		panic(err)
	}
	req.Header.Add("Content-Type", "application/json")
	req.SetBasicAuth("ada", "p'w\"d")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()
	out, _ := io.ReadAll(resp.Body)
	fmt.Println(resp.Status)
	fmt.Println(string(out))
}
