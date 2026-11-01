package main

import (
	"fmt"
	"io"
	"net/http"
)

func main() {
	req, err := http.NewRequest("PURGE", "https://cdn.example.test/assets/app.js", nil)
	if err != nil {
		panic(err)
	}
	req.Header.Add("X-Tag", "a")
	req.Header.Add("X-Tag", "b")
	req.SetBasicAuth("svc", "")

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()
	out, _ := io.ReadAll(resp.Body)
	fmt.Println(resp.Status)
	fmt.Println(string(out))
}
