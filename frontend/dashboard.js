;(function() {

const FORMATTERS = {
  datetime: new Intl.DateTimeFormat(navigator.language, {
    hour12: false,
    // year: "numeric",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "numeric",
    // second: "numeric",
  }),
  millisecond: new Intl.DateTimeFormat(navigator.language, {
    hour12: false,
    hour: "numeric",
    minute: "numeric",
    second: "numeric",
    fractionalSecondDigits: 3,
  }),
  second: new Intl.DateTimeFormat(navigator.language, {
    hour12: false,
    hour: "numeric",
    minute: "numeric",
    second: "numeric",
  }),
  minute: new Intl.DateTimeFormat(navigator.language, {
    hour12: false,
    hour: "numeric",
    minute: "numeric",
  }),
  hour: new Intl.DateTimeFormat(navigator.language, {
    hour12: false,
    hour: "numeric",
    minute: "numeric",
  }),
  day: new Intl.DateTimeFormat(navigator.language, {
    month: "short",
    day: "numeric",
  }),
  week: new Intl.DateTimeFormat(navigator.language, {
    month: "short",
    day: "numeric",
  }),
  month: new Intl.DateTimeFormat(navigator.language, {
    month: "short",
  }),
  quarter: new Intl.DateTimeFormat(navigator.language, {
    month: "short",
    year: "numeric",
  }),
  year: new Intl.DateTimeFormat(navigator.language, {
    year: "numeric",
  }),
};

const FORMATS = {};
for (let key of Object.keys(FORMATTERS)) {
  FORMATS[key] = key;
}

Chart._adapters._date.override({
  formats: () => FORMATS,
  format: function(time, format) {
    return FORMATTERS[format].format(new Date(time));
  },
  parse: function(value) {
    if (value instanceof Date)
      return value.getTime();
    if (typeof value === "number")
      return value;
    if (typeof value === "string")
      return value;
    throw new Error(`Not a date: ${JSON.stringify(value)}`);
  },
  add: function(t1, t2, unit) {
    switch (unit) {
    case 'millisecond':
      return t1 + t2;
    case 'second':
      return t1 + t2 * 1000;
    case 'minute':
      return t1 + t2 * 1000 * 60;
    case 'hour':
      return t1 + t2 * 1000 * 60 * 60;
    case 'day':
      return t1 + t2 * 1000 * 60 * 60 * 24;
    case 'week':
      return t1 + t2 * 1000 * 60 * 60 * 24 * 7;
    case 'month':
      return t1 + t2 * 1000 * 60 * 60 * 24 * 30;
    case 'quarter':
      return t1 + t2 * 1000 * 60 * 60 * 24 * 30 * 4;
    case 'year':
      return t1 + t2 * 1000 * 60 * 60 * 24 * 365;
    default:
      return t1;
    }
  },
  diff: function(t1, t2, unit) {
    const diff = t1 - t2;

    switch (unit) {
    case "millisecond":
      return diff;
    case "second":
      return diff / 1000;
    case "minute":
      return diff / 60000;
    case "hour":
      return diff / 3600000;
    case "day":
      return diff / 86400000;
    case "week":
    case "isoWeek":
      return diff / 604800000;
    case "month":
      return diff / 18144000000;
    default:
      return 0;
    }
  },
  startOf: function(time, unit) {
    const date = new Date(time);
    switch (unit) {
    case "millisecond":
      return time;
    case "second":
      return time - date.getMilliseconds();
    case "minute":
      return time
        - date.getMilliseconds()
        - (date.getSeconds() * 1000);
    case "hour":
      return time
        - date.getMilliseconds()
        - (date.getSeconds() * 1000)
        - (date.getMinutes() * 60000);
    case "day":
      return time
        - date.getMilliseconds()
        - (date.getSeconds() * 1000)
        - (date.getMinutes() * 60000)
        - (date.getHours() * 3600000);
    case "week":
    case "isoWeek":
      return time
        - date.getMilliseconds()
        - (date.getSeconds() * 1000)
        - (date.getMinutes() * 60000)
        - (date.getHours() * 3600000)
        - (date.getDay() * 86400000);
    case "month":
      return time
        - date.getMilliseconds()
        - (date.getSeconds() * 1000)
        - (date.getMinutes() * 60000)
        - (date.getHours() * 3600000)
        - (date.getDate() * 86400000);
    }
  },
  endOf: function(time, unit) {
    const startOf = this.startOf(time, unit);
    switch (unit) {
    case "millisecond":
      return time;
    case "day":
      return startOf + 3600000 * 24;
    case "week":
    case "isoWeek":
      return startOf + 3600000 * 24 * 7;
    case "month":
      return startOf + 3600000 * 24 * 30;
    }
  },
});

const WINDOW_SELECT = document.getElementById("window-select");

async function getMetrics(numSamples = 240, sampleWindow = "1h") {
  const metrics = await fetch(`/metrics?num_samples=${numSamples}&window=${sampleWindow}`).then(r => r.json());
  const accumulated = metrics.series.reduce((accum, series) => {
    if (accum.get(series.name) == null) {
      accum.set(series.name, []);
    }
    accum.get(series.name).push(series);
    return accum;
  }, new Map());
  const grouped = Array.from(accumulated.entries().map(([name, series]) => ({ name, series })));

  return {
    timestamps: metrics.timestamps,
    metrics: grouped,
  }
}

Chart.defaults.font.family = "sans-serif";

const CHARTS = window.CHARTS = new Map();

function datasets(timestamps, metric) {
  return metric.series.map(series => {
    const data = series.values.map((event, n) => ({
      x: timestamps[n],
      y: event,
    }));
    const dataset = { data };
    if (series.label) {
      dataset.label = series.label;
    }
    return dataset;
  });
}

function createChart(timestamps, metric) {
  const rootElement = document.getElementById("dashboards");
  const canvas = document.createElement("canvas");
  const container = document.createElement("div");
  container.classList.add("chart-container");
  container.appendChild(canvas)
  rootElement.appendChild(container);
  const ctx = canvas.getContext("2d");

  const chart = new Chart(ctx, {
    type: "line",
    normalized: true,
    parsing: false,
    data: {
      datasets: datasets(timestamps, metric),
    },
    options: {
      responsive: true, 
      animation: false,
      plugins: {
        title: {
          display: true,
          text: metric.name,
        },
        legend: {
          display: metric.series.some(s => s.label != null),
        },
      },
      elements: {
        point: {
          pointRadius: 1,
        },
      },
      scales: {
        x: {
          parser: false,
          type: "time",
          time: {
            // unit: "minute"
            // unit: TICK_UNITS[sampleWindow],
          },
          ticks: {
            // display: true,
            // source: "auto",
            major: {
              enabled: true,
            },
          },
        },
        y: {},
      },
    },
  });

  return chart;
}

async function render(sampleWindow) {
  const response = await getMetrics(240, sampleWindow);
  let metric = response.metrics[response.metrics.length - 1];
  for (let metric of response.metrics) {
    if (!metric.name) {
      console.warn("Ignoring metric with blank name");
      continue;
    }

    let chart = CHARTS.get(metric.name);
    if (chart == null) {
      chart = createChart(response.timestamps, metric);
      CHARTS.set(metric.name, chart);
    } else {
      chart.data.datasets = datasets(response.timestamps, metric);
      chart.update();
    }
  }
}

let RENDER_LOOP_TIMEOUT;

function renderLoop(sampleWindow) {
  render(sampleWindow || WINDOW_SELECT.value);
  RENDER_LOOP_TIMEOUT = setTimeout(renderLoop, 300000);
}

WINDOW_SELECT.addEventListener("change", event => {
  const sampleWindow = event.target.value;
  clearTimeout(RENDER_LOOP_TIMEOUT);
  renderLoop(sampleWindow);
});

renderLoop();

})();
