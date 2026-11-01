import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.HttpRequest;
import java.net.http.HttpResponse;

public class Main {
    public static void main(String[] args) throws Exception {
        HttpClient client = HttpClient.newHttpClient();
        HttpRequest request = HttpRequest.newBuilder()
            .uri(URI.create("https://api.example.test/profile?api_key={{key}}"))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .method("PUT", HttpRequest.BodyPublishers.ofString("name=Ada%20Lovelace&note=a%26b%3Dc%2F%C3%A9"))
            .build();
        HttpResponse<String> response = client.send(request, HttpResponse.BodyHandlers.ofString());
        System.out.println(response.statusCode());
        System.out.println(response.body());
    }
}
