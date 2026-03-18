# Eigen-Tune Setup Guide

Get self-tuning running on your machine. Takes 5 minutes.

---

## Prerequisites

Eigen-Tune needs two external tools. TEMM1E itself is pure Rust — these are only for the training and serving pipeline.

### 1. Ollama (Required — serves the fine-tuned model)

Ollama runs your fine-tuned model locally. Without it, Eigen-Tune still collects training data but cannot train or serve models.

**macOS:**
```bash
brew install ollama
ollama serve
```

**Linux:**
```bash
curl -fsSL https://ollama.com/install.sh | sh
ollama serve
```

**Windows:**
Download from https://ollama.com/download

**Verify:**
```bash
curl http://localhost:11434/
# Should print: "Ollama is running"
```

### 2. MLX (macOS Apple Silicon) or Unsloth (NVIDIA GPU) — for training

You need ONE of these, depending on your hardware.

**Apple Silicon (M1/M2/M3/M4):**
```bash
# Requires Python 3.10+
python3 -m pip install mlx-lm

# Verify:
python3 -c "import mlx_lm; print('MLX ready')"
```

If your system Python is too old, use Homebrew:
```bash
brew install python@3.12
python3.12 -m venv ~/.eigentune-env
source ~/.eigentune-env/bin/activate
pip install mlx-lm
```

**NVIDIA GPU (Linux/Windows):**
```bash
pip install unsloth

# Verify:
python3 -c "import unsloth; print('Unsloth ready')"
```

**No GPU?** Eigen-Tune still collects and scores training data. When you get access to a GPU (or a cloud GPU burst), the data is ready to train.

---

## Enable Eigen-Tune

Add to your `temm1e.toml`:

```toml
[eigentune]
enabled = true
```

That's it. Restart TEMM1E and the system begins collecting training data from every conversation.

---

## Choose a Base Model (Optional)

By default, Eigen-Tune auto-selects the best model for your hardware. To see options or set a specific model:

```
/eigentune model              Show available models for your hardware
/eigentune model auto         Let system pick (default)
/eigentune model <name>       Set a specific model
```

**Recommendations by hardware:**

| RAM | Recommended | Why |
|-----|------------|-----|
| 8 GB | SmolLM2-135M | Fastest training, proof of concept |
| 8 GB | Qwen2.5-0.5B | Slightly better quality |
| 16 GB | Qwen2.5-1.5B | Good balance of speed and quality |
| 16 GB | Llama-3.1-8B | Maximum capability for 16 GB |
| 32 GB | Mistral-Small-24B | Strong quality, needs more RAM |
| 48 GB+ | Qwen2.5-32B | Best possible local model |

---

## Check Status

```
/eigentune status             Full status: data, tiers, model, prerequisites
/eigentune model              Model selection and hardware info
```

---

## What Happens Next

1. **Collecting** — every conversation produces training pairs, automatically scored
2. **Training** — when enough quality data accumulates (500+ pairs per tier), training triggers automatically
3. **Evaluating** — the trained model is tested against your actual conversation patterns
4. **Graduating** — if it passes statistical gates (95% accuracy, 99% confidence), it starts serving simple queries locally
5. **Monitoring** — continuous drift detection, auto-demotion if quality drops

You don't need to do anything. The system handles the entire lifecycle. Cloud is always the fallback — if anything goes wrong, your experience is identical to before Eigen-Tune existed.

---

## Troubleshooting

**"No training backend available"**
- Install MLX (Apple Silicon) or Unsloth (NVIDIA GPU)
- Eigen-Tune still collects data without a training backend

**"Ollama not running"**
- Run `ollama serve` in a separate terminal
- Or set up as a system service: `brew services start ollama` (macOS)

**"Python not found" or "mlx_lm not found"**
- Ensure Python 3.10+ is installed
- If using a venv, activate it before starting TEMM1E
- Or install globally: `pip install mlx-lm`

**"Not enough data"**
- Eigen-Tune needs ~500 quality conversations per tier before first training
- Keep using TEMM1E normally — data accumulates automatically
- Check progress: `/eigentune status`

---

## What Eigen-Tune Does NOT Do

- Does NOT send your data anywhere — all training is local
- Does NOT require GPU for data collection — only for training
- Does NOT modify your existing conversations or provider behavior
- Does NOT cost any additional LLM API money
- Does NOT replace your cloud provider — it gradually supplements it for simple queries
