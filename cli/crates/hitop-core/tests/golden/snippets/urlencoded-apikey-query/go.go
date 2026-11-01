package main

import (
	"fmt"
	"io"
	"net/http"
	"strings"
)

func main() {
	body := strings.NewReader("name=Ada%20Lovelace&note=a%26b%3Dc%2F%C3%A9")
	req, err := http.NewRequest("PUT", "https://api.example.test/profile?api_key={{key}}", body)
	if err != nil {
		panic(err)
	}
	req.Header.Add("Content-Type", "application/x-www-form-urlencoded")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()
	out, _ := io.ReadAll(resp.Body)
	fmt.Println(resp.Status)
	fmt.Println(string(out))
}
