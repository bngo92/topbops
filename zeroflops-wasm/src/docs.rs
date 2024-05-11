use yew::{html, Html};

pub fn docs() -> Html {
    crate::nav_content(
        html! {
          <ul class="navbar-nav me-auto">
            <li class="navbar-brand">{"Docs"}</li>
          </ul>
        },
        html! {
          <div>
            <h3>{"Item features"}</h3>
            <h5>{"Rank items"}</h5>
            <p>{"Rank items like your songs to figure out what your favorite songs are."}</p>
            <h5>{"Rate items"}</h5>
            <p>{"Item ratings provide a method for ranking/grouping your items using a scale of 1 to 10."}</p>
            <h5>{"Query items"}</h5>
            <p>{"Query using SQL to gain insights about your data by calculating statistics and filtering items.
                You can also view your data using different types of charts.
                The first column returned by the SQL query is used as the x-axis and the second column is used as the y."}</p>
            <h5>{"Manage items"}</h5>
            <p>{"You can mark items as hidden for queries to filter on.
                You can also delete items to remove it from all lists and queries."}</p>
            <h3>{"List features"}</h3>
            <h5>{"Create lists of items using data sources"}</h5>
            <p>{"Add items to a list by adding a data source that resolves to items.
                Deleting a data source will remove the items from the list but the data for items will still be preserved."}</p>
            <h5>{"Query items in a list"}</h5>
            <p>{"Queries under a list page are similar to queries in the top-level page except they will also be filtered against items in the list."}</p>
            <h5>{"Integrate with external systems"}</h5>
            <p>{"If the data sources of your list support external integrations, you can define an ID that will be used for the integration.
                For example, lists that only use Spotify data sources can push the items into a Spotify playlist with the given ID."}</p>
            <h5>{"Define a default query for the list"}</h5>
            <p>{"The query will be used as the default query for the query view and any push actions."}</p>
            <h5>{"Favorite lists"}</h5>
            <p>{"Favorite lists will show up on the home page along with results from the default query."}</p>
            <h3>{"Combined features"}</h3>
            <p>{"Create a Spotify playlist from other Spotify albums and playlists."}</p>
            <p>{"Create a Spotify playlist from songs that you've rated 7 or above."}</p>
            <p>{"Create a list from other lists to reuse the data sources."}</p>
            <p>{"Create a table showing your average ratings by album."}</p>
            <p>{"Create a column chart showing artists with the most number of songs you've rated 10 out of 10."}</p>
          </div>
        },
    )
}
