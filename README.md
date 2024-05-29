# MyOccupancy Backend

This is the backend for the MyOccupancy Project. Written completely in Rust, it has two core features:

1. Custom built but easily extendable web scraper 
2. Multi-Threaded web server exposing API endpoints

## Technologies

- The [tokio](https://tokio.rs/) runtime has been used for easy concurrency and multi-threading. 
- The [sqlite](https://www.sqlite.org/) database with the [`rusqlite`](https://github.com/rusqlite/rusqlite) library.
- [`hyper.rs`](https://hyper.rs/) as the HTTP library to send HTTP responses as the server.
- [`reqwest`](https://github.com/seanmonstar/reqwest) as the HTTP library to send HTTP requests as the client (for web scraping).

## Web Scraper

The Web Scraper is very extendible as you can create your own scrapers and add
them to the tokio runtime and have them run concurrently in the background.

Simply create a struct for each of your webscrapers and implement the Scrape
trait. Then add it in the Scraper struct's run method.

## The Server

The server accepts all TCP requests and creates a tokio thread to server it.
This features several endpoints for use in the frontend side of things.
