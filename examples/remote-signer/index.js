import express from "express";
import bodyParser from "body-parser";
import { Wallet } from "ethers";
import dotenv from "dotenv";

dotenv.config();

const PORT = process.env.PORT || 3001;
const app = express();
app.use(bodyParser.json());

if (!process.env.SIGNER_PRIVATE_KEY) {
  console.error("Please set SIGNER_PRIVATE_KEY in .env for the example remote signer");
  process.exit(1);
}

const wallet = new Wallet(process.env.SIGNER_PRIVATE_KEY);

app.get("/address", (_req, res) => {
  res.json({ address: wallet.address });
});

app.post("/sign", async (req, res) => {
  try {
    const { tx } = req.body;
    if (!tx) return res.status(400).json({ error: "tx missing" });
    // Expect tx to be a serialized TransactionRequest-like object
    const signed = await wallet.signTransaction(tx);
    res.json({ signedTx: signed });
  } catch (err) {
    console.error(err);
    res.status(500).json({ error: String(err) });
  }
});

app.listen(PORT, () => console.log(`Remote signer listening on ${PORT}`));
