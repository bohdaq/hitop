package main

import (
	"bytes"
	"fmt"
	"io"
	"mime/multipart"
	"net/http"
	"os"
)

func main() {
	var body bytes.Buffer
	form := multipart.NewWriter(&body)
	form.WriteField("title", "@not-a-file")
	file1, err := os.Open("photo.png")
	if err != nil {
		panic(err)
	}
	part1, _ := form.CreateFormFile("avatar", "photo.png")
	io.Copy(part1, file1)
	file1.Close()
	form.Close()
	req, err := http.NewRequest("POST", "https://api.example.test/upload", &body)
	if err != nil {
		panic(err)
	}
	req.Header.Add("X-API-Key", "k-123")
	req.Header.Set("Content-Type", form.FormDataContentType())

	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		panic(err)
	}
	defer resp.Body.Close()
	out, _ := io.ReadAll(resp.Body)
	fmt.Println(resp.Status)
	fmt.Println(string(out))
}
