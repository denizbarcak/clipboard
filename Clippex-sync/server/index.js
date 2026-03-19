require("dotenv").config();
const express = require("express");
const cors = require("cors");
const routes = require("./routes");

const app = express();
const PORT = process.env.PORT || 3456;

app.use(cors());
app.use(express.json());

// API routes
app.use("/api", routes);

// Health check
app.get("/health", (req, res) => {
  res.json({ status: "ok", service: "Clippex API" });
});

app.listen(PORT, () => {
  console.log(`Clippex API çalışıyor: http://localhost:${PORT}`);
  console.log(`Health check: http://localhost:${PORT}/health`);
});
